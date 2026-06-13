use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::constants::{DELIVERIES_PER_PAGE, MAX_MODEL_ID_LEN, MAX_MODELS, MAX_VOTES_PER_HOUR};
use crate::error::AppError;
use crate::extractors::{extract_client_ip, is_valid_model_id, AdminAuth, MAX_CLIENT_UUID_LEN};
use crate::hf;
use crate::models::{
    CreateModelRequest, CreateModelResponse, DeliveryRequest, LeaderboardEntry,
    PaginatedDeliveries, UpdateModelRequest, VoteRequest, VoteResponse,
};
use crate::queries;
use crate::state::AppState;

/// Validate a model ID: non-empty, within length, safe characters.
fn validate_model_id(id: &str) -> Result<String, AppError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(AppError::BadRequest("model_id is required".to_string()));
    }
    if id.len() > MAX_MODEL_ID_LEN {
        return Err(AppError::BadRequest(format!(
            "model_id must be {MAX_MODEL_ID_LEN} characters or less"
        )));
    }
    if !is_valid_model_id(&id) {
        return Err(AppError::BadRequest(
            "model_id contains invalid characters (allowed: alphanumeric, - _ . /)".to_string(),
        ));
    }
    Ok(id)
}

/// GET /api/leaderboard
///
/// Returns all models ordered by vote count as JSON.
pub async fn get_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<Vec<LeaderboardEntry>>, AppError> {
    let entries = queries::fetch_leaderboard(&state.db).await?;
    Ok(Json(entries))
}

/// Pagination and sorting query parameters.
#[derive(Debug, serde::Deserialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub sort: Option<String>,
}

impl PaginationParams {
    pub fn delivery_sort(&self) -> queries::DeliverySort {
        match self.sort.as_deref() {
            Some("kl") => queries::DeliverySort::KlDivergence,
            Some("refusal") => queries::DeliverySort::Refusal,
            _ => queries::DeliverySort::Date,
        }
    }
}

/// GET /api/deliveries?page=1&per_page=10&sort=date
///
/// Returns paginated deliveries as JSON. Sort options: `date`, `kl`, `refusal`.
pub async fn get_deliveries(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedDeliveries>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(DELIVERIES_PER_PAGE).clamp(1, 100);
    let offset = (page - 1) * per_page;
    let sort = params.delivery_sort();

    let total = queries::count_deliveries(&state.db).await?;
    let total_pages = (total + per_page - 1) / per_page; // ceiling division
    let items = queries::fetch_deliveries(&state.db, per_page, offset, sort).await?;

    Ok(Json(PaginatedDeliveries {
        items,
        page,
        per_page,
        total,
        total_pages,
    }))
}

/// POST /api/vote
///
/// Records a vote from a client for a model. Enforces rate limits:
/// - One vote per client per model (deduplication by UUID).
/// - One vote per model per IP per hour.
/// - Max 3 votes per IP per hour.
///   Auto-creates the model if it doesn't exist.
pub async fn post_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<VoteRequest>,
) -> Result<(axum::http::StatusCode, Json<VoteResponse>), AppError> {
    if payload.client_uuid.is_empty() {
        return Err(AppError::BadRequest(
            "client_uuid is required".to_string(),
        ));
    }
    if payload.client_uuid.len() > MAX_CLIENT_UUID_LEN {
        return Err(AppError::BadRequest(format!(
            "client_uuid must be {MAX_CLIENT_UUID_LEN} characters or less"
        )));
    }

    let model_id = validate_model_id(&payload.model_id)?;
    let ip = extract_client_ip(&headers, Some(addr));

    let mut tx = state.db.begin().await?;

    if queries::has_voted(&mut *tx, &payload.client_uuid, &model_id).await? {
        return Err(AppError::Conflict(
            "You have already voted for this model".to_string(),
        ));
    }

    if queries::has_ip_voted_for_model(&mut *tx, &ip, &model_id).await? {
        return Err(AppError::TooManyRequests(
            "Rate limit: This IP already voted for this model in the last hour".to_string(),
        ));
    }

    let ip_votes = queries::count_ip_votes(&mut *tx, &ip).await?;
    if ip_votes >= MAX_VOTES_PER_HOUR {
        return Err(AppError::TooManyRequests(format!(
            "Rate limit: Maximum {MAX_VOTES_PER_HOUR} votes per hour per IP"
        )));
    }

    if !queries::model_exists(&mut *tx, &model_id).await? {
        // Reject if this model has already been abliterated.
        if queries::has_been_delivered(&mut *tx, &model_id).await? {
            return Err(AppError::Conflict(
                "This model has already been abliterated".to_string(),
            ));
        }
        // Verify the model exists on HuggingFace before accepting it.
        if state.validate_hf {
            hf::validate_model_on_hf(&state.http_client, &model_id).await?;
        }
        queries::create_model(&mut *tx, &model_id, "auto").await?;
    }

    queries::record_vote(&mut tx, &payload.client_uuid, &model_id, &ip).await?;

    tx.commit().await?;

    tracing::info!(
        client_uuid = %payload.client_uuid,
        model_id = %model_id,
        ip = %ip,
        "Vote recorded"
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(VoteResponse {
            message: "Vote recorded successfully".to_string(),
        }),
    ))
}

/// POST /api/models
///
/// Creates a new model from user submission. Enforces:
/// - Non-empty model ID, max 100 characters, safe characters.
/// - Max 50 models on the board.
/// - One model creation per IP per day.
pub async fn post_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<CreateModelRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateModelResponse>), AppError> {
    let model_id = validate_model_id(&payload.model_id)?;
    let ip = extract_client_ip(&headers, Some(addr));

    let model_count = queries::count_models(&state.db).await?;
    if model_count >= MAX_MODELS {
        return Err(AppError::Forbidden(
            "Maximum 50 models allowed on the board".to_string(),
        ));
    }

    if queries::model_exists(&state.db, &model_id).await? {
        return Err(AppError::Conflict("Model already exists".to_string()));
    }

    if queries::has_been_delivered(&state.db, &model_id).await? {
        return Err(AppError::Conflict(
            "This model has already been abliterated".to_string(),
        ));
    }

    if queries::has_recent_model_creation(&state.db, &ip).await? {
        return Err(AppError::TooManyRequests(
            "Rate limit: You can only add 1 model per day".to_string(),
        ));
    }

    // Verify the model exists on HuggingFace before accepting it.
    if state.validate_hf {
        hf::validate_model_on_hf(&state.http_client, &model_id).await?;
    }

    queries::create_model(&state.db, &model_id, &ip).await?;

    tracing::info!(model_id = %model_id, ip = %ip, "Model created");
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateModelResponse {
            message: "Model submitted for voting".to_string(),
            model_id,
        }),
    ))
}

/// POST /api/admin/models
///
/// Admin endpoint to create a model with an optional HF link.
/// Validates the HF link against the HuggingFace API if provided.
pub async fn admin_create_model(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(payload): Json<CreateModelRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateModelResponse>), AppError> {
    let model_id = validate_model_id(&payload.model_id)?;

    let model_count = queries::count_models(&state.db).await?;
    if model_count >= MAX_MODELS {
        return Err(AppError::Forbidden(
            "Maximum 50 models allowed on the board".to_string(),
        ));
    }

    if queries::model_exists(&state.db, &model_id).await? {
        return Err(AppError::Conflict("Model already exists".to_string()));
    }

    if queries::has_been_delivered(&state.db, &model_id).await? {
        return Err(AppError::Conflict(
            "This model has already been abliterated".to_string(),
        ));
    }

    // Verify the model exists on HuggingFace.
    if state.validate_hf {
        hf::validate_model_on_hf(&state.http_client, &model_id).await?;
    }

    if let Some(ref hf_link) = payload.hf_link
        && !hf_link.trim().is_empty()
    {
        hf::validate_hf_link(&state.http_client, hf_link).await?;
    }

    queries::create_model_with_hf(&state.db, &model_id, &payload.hf_link).await?;

    tracing::info!(model_id = %model_id, "Admin created model");
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateModelResponse {
            message: "Model created successfully".to_string(),
            model_id,
        }),
    ))
}

/// PUT /api/admin/models/:model_id
///
/// Admin endpoint to update a model's HF link. Validates the link
/// against the HuggingFace API.
pub async fn admin_update_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
    _auth: AdminAuth,
    Json(payload): Json<UpdateModelRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !queries::model_exists(&state.db, &model_id).await? {
        return Err(AppError::NotFound("Model not found".to_string()));
    }

    let hf_link = payload.hf_link.ok_or_else(|| {
        AppError::BadRequest("hf_link is required".to_string())
    })?;
    let hf_link = hf_link.trim().to_string();
    if hf_link.is_empty() {
        return Err(AppError::BadRequest(
            "hf_link must not be empty".to_string(),
        ));
    }

    hf::validate_hf_link(&state.http_client, &hf_link).await?;
    queries::update_model_hf_link(&state.db, &hf_link, &model_id).await?;

    tracing::info!(model_id = %model_id, "Admin updated model");
    Ok(Json(serde_json::json!({
        "message": "Model updated successfully",
        "model_id": model_id,
    })))
}

/// POST /api/admin/deliver
///
/// Admin endpoint to record that a model has been abliterated.
/// The model must already exist on the board. Requires abliteration
/// metrics: `kl_divergence`, `refused`, `total_prompts`.
pub async fn admin_deliver(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(payload): Json<DeliveryRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    let model_id = validate_model_id(&payload.model_id)?;

    let hf_link = payload.hf_link.trim().to_string();
    if hf_link.is_empty() {
        return Err(AppError::BadRequest(
            "hf_link is required".to_string(),
        ));
    }
    if !hf_link.starts_with("https://") {
        return Err(AppError::BadRequest(
            "hf_link must be an HTTPS URL".to_string(),
        ));
    }

    if payload.kl_divergence < 0.0 {
        return Err(AppError::BadRequest(
            "kl_divergence must be non-negative".to_string(),
        ));
    }
    if payload.refused < 0 || payload.total_prompts <= 0 {
        return Err(AppError::BadRequest(
            "refused must be >= 0 and total_prompts must be > 0".to_string(),
        ));
    }
    if payload.refused > payload.total_prompts {
        return Err(AppError::BadRequest(
            "refused cannot exceed total_prompts".to_string(),
        ));
    }

    let (model_id_str, vote_count) =
        queries::find_model_for_delivery(&state.db, &model_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Model not found".to_string()))?;

    queries::insert_delivery(
        &state.db,
        &model_id_str,
        vote_count,
        &hf_link,
        &payload.notes,
        payload.kl_divergence,
        payload.refused,
        payload.total_prompts,
    )
    .await?;

    // Remove the model from the leaderboard now that it's been delivered.
    // Votes are cascade-deleted by the FK constraint.
    queries::delete_model(&state.db, &model_id_str).await?;

    tracing::info!(
        model_id = %model_id_str,
        vote_count = vote_count,
        kl_divergence = payload.kl_divergence,
        refused = payload.refused,
        total_prompts = payload.total_prompts,
        "Delivery recorded"
    );
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "Delivery recorded successfully",
            "model_id": model_id_str,
        })),
    ))
}

/// DELETE /api/admin/models/:model_id
///
/// Admin endpoint to delete a model and its votes.
pub async fn admin_delete_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
    _auth: AdminAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = queries::delete_model(&state.db, &model_id).await?;
    if !deleted {
        return Err(AppError::NotFound("Model not found".to_string()));
    }

    tracing::info!(model_id = %model_id, "Admin deleted model");
    Ok(Json(serde_json::json!({
        "message": "Model deleted successfully",
        "model_id": model_id,
    })))
}

/// DELETE /api/admin/prune
///
/// Admin endpoint to prune all models with zero votes.
pub async fn admin_prune_models(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = queries::prune_zero_vote_models(&state.db).await?;

    tracing::info!(deleted = deleted, "Admin pruned zero-vote models");
    Ok(Json(serde_json::json!({
        "message": format!("{deleted} model(s) pruned"),
        "deleted": deleted,
    })))
}

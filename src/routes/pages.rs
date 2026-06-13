use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Html;

use crate::constants::{DELIVERIES_PER_PAGE, MAX_MODEL_ID_LEN, MAX_MODELS, MAX_VOTES_PER_HOUR};
use crate::error::AppError;
use crate::extractors::{extract_client_ip, is_valid_model_id, MAX_CLIENT_UUID_LEN};
use crate::hf;
use crate::models::{CreateModelRequest, VoteRequest};
use crate::queries;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Template types
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    models: Vec<crate::models::LeaderboardEntry>,
    deliveries: Vec<crate::models::DeliveryEntry>,
    total_votes: i64,
    total_deliveries: i64,
    page: i64,
    total_pages: i64,
    sort: String,
}

#[derive(Template)]
#[template(path = "leaderboard_table.html")]
struct LeaderboardTableTemplate {
    models: Vec<crate::models::LeaderboardEntry>,
}

#[derive(Template)]
#[template(path = "deliveries_list.html")]
struct DeliveriesListTemplate {
    deliveries: Vec<crate::models::DeliveryEntry>,
    page: i64,
    total_pages: i64,
    sort: String,
}

#[derive(Template)]
#[template(path = "action_result.html")]
struct ActionResultTemplate {
    message: String,
    success: bool,
}

#[derive(Template)]
#[template(path = "model_select.html")]
struct ModelSelectTemplate {
    models: Vec<crate::models::LeaderboardEntry>,
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Render an `ActionResultTemplate` as an HTML response.
fn action_result(message: impl Into<String>, success: bool) -> Result<Html<String>, AppError> {
    let tpl = ActionResultTemplate {
        message: message.into(),
        success,
    };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

// ---------------------------------------------------------------------------
// Page handlers
// ---------------------------------------------------------------------------

/// GET /
///
/// Main page with hero, stats, leaderboard, and deliveries.
pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let total_deliveries = queries::count_deliveries(&state.db).await?;
    let deliveries = queries::fetch_deliveries(
        &state.db, DELIVERIES_PER_PAGE, 0, queries::DeliverySort::Date,
    ).await?;
    let total_votes = models.iter().map(|m| m.vote_count).sum();
    let total_pages = (total_deliveries + DELIVERIES_PER_PAGE - 1) / DELIVERIES_PER_PAGE;
    let tpl = IndexTemplate {
        models,
        deliveries,
        total_votes,
        total_deliveries,
        page: 1,
        total_pages: total_pages.max(1),
        sort: "date".into(),
    };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /leaderboard/table
///
/// HTMX partial: leaderboard table rows.
pub async fn leaderboard_table(
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let tpl = LeaderboardTableTemplate { models };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /deliveries/list?page=1&sort=date
///
/// HTMX partial: paginated abliterated model items with sort controls.
pub async fn deliveries_list(
    State(state): State<AppState>,
    Query(params): Query<crate::routes::api::PaginationParams>,
) -> Result<Html<String>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(DELIVERIES_PER_PAGE).clamp(1, 100);
    let offset = (page - 1) * per_page;
    let sort = params.delivery_sort();
    let sort_str = params.sort.clone().unwrap_or_else(|| "date".into());

    let total = queries::count_deliveries(&state.db).await?;
    let total_pages = (total + per_page - 1) / per_page;
    let deliveries = queries::fetch_deliveries(&state.db, per_page, offset, sort).await?;

    let tpl = DeliveriesListTemplate {
        deliveries,
        page,
        total_pages,
        sort: sort_str,
    };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /model-select
///
/// HTMX partial: `<option>` elements for the vote dropdown.
pub async fn model_select(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let tpl = ModelSelectTemplate { models };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// POST /vote
///
/// Form-based vote submission (HTMX). Returns an `ActionResult` fragment.
pub async fn vote_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Form(payload): axum::extract::Form<VoteRequest>,
) -> Result<Html<String>, AppError> {
    if payload.client_uuid.is_empty() || payload.model_id.is_empty() {
        return action_result("client_uuid and model_id are required", false);
    }
    if payload.client_uuid.len() > MAX_CLIENT_UUID_LEN {
        return action_result("Invalid client_uuid", false);
    }
    if !is_valid_model_id(payload.model_id.trim()) {
        return action_result("Model name contains invalid characters", false);
    }

    let ip = extract_client_ip(&headers, Some(addr));
    let mut tx = state.db.begin().await?;

    if queries::has_voted(&mut *tx, &payload.client_uuid, &payload.model_id).await? {
        return action_result("You have already voted for this model", false);
    }

    if queries::has_ip_voted_for_model(&mut *tx, &ip, &payload.model_id).await? {
        return action_result(
            "Rate limit: This IP already voted for this model in the last hour",
            false,
        );
    }

    let ip_votes = queries::count_ip_votes(&mut *tx, &ip).await?;
    if ip_votes >= MAX_VOTES_PER_HOUR {
        return action_result(
            format!("Rate limit: Maximum {MAX_VOTES_PER_HOUR} votes per hour per IP"),
            false,
        );
    }

    if !queries::model_exists(&mut *tx, &payload.model_id).await? {
        if queries::has_been_delivered(&mut *tx, &payload.model_id).await? {
            return action_result("This model has already been abliterated", false);
        }
        // Verify the model exists on HuggingFace before accepting it.
        if state.validate_hf
            && let Err(e) = hf::validate_model_on_hf(&state.http_client, &payload.model_id).await
        {
            return action_result(e.to_string(), false);
        }
        queries::create_model(&mut *tx, &payload.model_id, "auto").await?;
    }

    queries::record_vote(&mut tx, &payload.client_uuid, &payload.model_id, &ip).await?;
    tx.commit().await?;

    action_result("Vote recorded successfully", true)
}

/// POST /add-model
///
/// Form-based model submission (HTMX). Returns an `ActionResult` fragment.
pub async fn create_model_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Form(payload): axum::extract::Form<CreateModelRequest>,
) -> Result<Html<String>, AppError> {
    let ip = extract_client_ip(&headers, Some(addr));
    let model_id = payload.model_id.trim().to_string();

    if model_id.is_empty() {
        return action_result("Model name is required", false);
    }
    if model_id.len() > MAX_MODEL_ID_LEN {
        return action_result(
            format!("Model name must be {MAX_MODEL_ID_LEN} characters or less"),
            false,
        );
    }
    if !is_valid_model_id(&model_id) {
        return action_result(
            "Model name contains invalid characters (allowed: alphanumeric, - _ . /)",
            false,
        );
    }

    let model_count = queries::count_models(&state.db).await?;
    if model_count >= MAX_MODELS {
        return action_result(
            "Maximum 50 models allowed in the leaderboard",
            false,
        );
    }

    if queries::model_exists(&state.db, &model_id).await? {
        return action_result("Model already exists", false);
    }

    if queries::has_been_delivered(&state.db, &model_id).await? {
        return action_result("This model has already been abliterated", false);
    }

    if queries::has_recent_model_creation(&state.db, &ip).await? {
        return action_result(
            "Rate limit: You can only add 1 model per day",
            false,
        );
    }

    // Verify the model exists on HuggingFace before accepting it.
    if state.validate_hf
        && let Err(e) = hf::validate_model_on_hf(&state.http_client, &model_id).await
    {
        return action_result(e.to_string(), false);
    }

    queries::create_model(&state.db, &model_id, &ip).await?;

    action_result(format!("Model '{model_id}' submitted for voting"), true)
}

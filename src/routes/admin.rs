use askama::Template;
use axum::extract::State;
use axum::response::Html;

use crate::error::AppError;
use crate::extractors::AdminAuth;
use crate::models::{DeliveryEntry, LeaderboardEntry};
use crate::queries;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Template types
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTemplate {
    models: Vec<LeaderboardEntry>,
    deliveries: Vec<DeliveryEntry>,
    total_deliveries: i64,
    total_votes: i64,
}

#[derive(Template)]
#[template(path = "admin_models.html")]
struct AdminModelsTemplate {
    models: Vec<LeaderboardEntry>,
}

#[derive(Template)]
#[template(path = "admin_deliveries.html")]
struct AdminDeliveriesTemplate {
    deliveries: Vec<DeliveryEntry>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /admin
///
/// Admin dashboard. Requires `AdminAuth` extractor (401 if unauthenticated).
pub async fn admin_page(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let total_deliveries = queries::count_deliveries(&state.db).await?;
    let deliveries = queries::fetch_deliveries(
        &state.db, 100, 0, queries::DeliverySort::Date,
    ).await?;
    let total_votes = models.iter().map(|m| m.vote_count).sum::<i64>();

    let tpl = AdminTemplate {
        models,
        deliveries,
        total_deliveries,
        total_votes,
    };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /admin/login
///
/// Public login form. No authentication required.
pub async fn admin_login_page() -> Result<Html<String>, AppError> {
    let tpl = AdminLoginTemplate;
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// POST /admin/login
///
/// Validates the admin token via `AdminAuth` extractor.
/// Returns the full dashboard HTML. Client stores token in localStorage
/// for subsequent HTMX requests.
pub async fn admin_login(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let total_deliveries = queries::count_deliveries(&state.db).await?;
    let deliveries = queries::fetch_deliveries(
        &state.db, 100, 0, queries::DeliverySort::Date,
    ).await?;
    let total_votes = models.iter().map(|m| m.vote_count).sum::<i64>();

    let tpl = AdminTemplate {
        models,
        deliveries,
        total_deliveries,
        total_votes,
    };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /admin/models
///
/// HTMX partial: model rows for the admin table. Requires `AdminAuth`.
pub async fn admin_models(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Html<String>, AppError> {
    let models = queries::fetch_leaderboard(&state.db).await?;
    let tpl = AdminModelsTemplate { models };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

/// GET /admin/deliveries
///
/// HTMX partial: delivery rows for the admin table. Requires `AdminAuth`.
pub async fn admin_deliveries(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Html<String>, AppError> {
    let deliveries = queries::fetch_deliveries(
        &state.db, 100, 0, queries::DeliverySort::Date,
    ).await?;
    let tpl = AdminDeliveriesTemplate { deliveries };
    Ok(Html(tpl.render().map_err(|e| AppError::Internal(e.to_string()))?))
}

// ---------------------------------------------------------------------------
// Login form template
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin_login.html")]
struct AdminLoginTemplate;
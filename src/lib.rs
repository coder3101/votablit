pub mod constants;
pub mod db;
pub mod error;
pub mod extractors;
pub mod hf;
pub mod models;
pub mod queries;
pub mod routes;
pub mod state;

use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::services::ServeDir;

use crate::routes::*;
use crate::state::AppState;

/// Build the application router with all routes and middleware.
///
/// This is public so integration tests can construct the router without
/// starting a TCP listener.
pub fn app(state: AppState) -> Router {
    Router::new()
        // HTML pages (Askama + HTMX)
        .route("/", get(pages::index))
        .route("/leaderboard/table", get(pages::leaderboard_table))
        .route("/deliveries/list", get(pages::deliveries_list))
        .route("/model-select", get(pages::model_select))
        .route("/vote", post(pages::vote_page))
        .route("/add-model", post(pages::create_model_page))
        // JSON API
        .route("/api/leaderboard", get(api::get_leaderboard))
        .route("/api/deliveries", get(api::get_deliveries))
        .route("/api/vote", post(api::post_vote))
        .route("/api/models", post(api::post_model))
        // Admin API
        .route("/api/admin/models", post(api::admin_create_model))
        .route(
            "/api/admin/models/{model_id}",
            put(api::admin_update_model).delete(api::admin_delete_model),
        )
        .route("/api/admin/prune", delete(api::admin_prune_models))
        .route("/api/admin/deliver", post(api::admin_deliver))
        // Static files
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}

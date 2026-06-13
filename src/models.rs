use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Database row types (used with `sqlx::query_as!`)
//
// NOTE: `query_as!` maps columns to struct fields by name at compile time —
// it does NOT use the `FromRow` derive.  The derive is intentionally omitted.
// ---------------------------------------------------------------------------

/// Row type for the `models` table.
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub model_id: String,
    pub vote_count: i64,
    pub hf_link: Option<String>,
}

/// Row type for the `deliveries` table.
#[derive(Debug, Clone)]
pub struct DeliveryRow {
    pub id: i64,
    pub model_id: String,
    pub vote_count: i64,
    pub hf_link: String,
    pub notes: Option<String>,
    pub kl_divergence: f64,
    pub refused: i64,
    pub total_prompts: i64,
    pub delivered_at: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// API request payloads
// ---------------------------------------------------------------------------

/// Payload for casting a vote.
#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    pub client_uuid: String,
    pub model_id: String,
}

/// Payload for creating a model (user or admin).
#[derive(Debug, Deserialize)]
pub struct CreateModelRequest {
    pub model_id: String,
    pub hf_link: Option<String>,
}

/// Payload for updating a model (admin only).
#[derive(Debug, Deserialize)]
pub struct UpdateModelRequest {
    pub hf_link: Option<String>,
}

/// Payload for recording a delivery (admin only).
#[derive(Debug, Deserialize)]
pub struct DeliveryRequest {
    pub model_id: String,
    pub hf_link: String,
    pub notes: Option<String>,
    pub kl_divergence: f64,
    pub refused: i64,
    pub total_prompts: i64,
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// A single entry in the leaderboard JSON response.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub model_id: String,
    pub vote_count: i64,
    pub hf_link: Option<String>,
}

/// A single entry in the deliveries JSON response.
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryEntry {
    pub id: i64,
    pub model_id: String,
    pub vote_count: i64,
    pub hf_link: String,
    pub notes: Option<String>,
    pub kl_divergence: f64,
    pub refused: i64,
    pub total_prompts: i64,
    pub delivered_at: String,
}

/// Successful vote response.
#[derive(Debug, Serialize)]
pub struct VoteResponse {
    pub message: String,
}

/// Successful model creation response.
#[derive(Debug, Serialize)]
pub struct CreateModelResponse {
    pub message: String,
    pub model_id: String,
}

/// Paginated deliveries response.
#[derive(Debug, Serialize)]
pub struct PaginatedDeliveries {
    pub items: Vec<DeliveryEntry>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

// ---------------------------------------------------------------------------
// Conversions from row types to API types
// ---------------------------------------------------------------------------

impl From<ModelRow> for LeaderboardEntry {
    fn from(row: ModelRow) -> Self {
        Self {
            model_id: row.model_id,
            vote_count: row.vote_count,
            hf_link: row.hf_link,
        }
    }
}

impl From<DeliveryRow> for DeliveryEntry {
    fn from(row: DeliveryRow) -> Self {
        Self {
            id: row.id,
            model_id: row.model_id,
            vote_count: row.vote_count,
            hf_link: row.hf_link,
            notes: row.notes,
            kl_divergence: row.kl_divergence,
            refused: row.refused,
            total_prompts: row.total_prompts,
            delivered_at: row.delivered_at.format("%Y-%m-%d").to_string(),
        }
    }
}

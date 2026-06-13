/// Maximum number of models allowed in the leaderboard.
pub const MAX_MODELS: i64 = 50;

/// Maximum characters allowed for a model ID.
pub const MAX_MODEL_ID_LEN: usize = 100;

/// Maximum number of votes per IP address per hour.
pub const MAX_VOTES_PER_HOUR: i64 = 3;

/// Maximum allowed model size in bytes (70 GB).
pub const MAX_MODEL_SIZE_BYTES: u64 = 70 * 1024 * 1024 * 1024;

/// Timeout for HuggingFace API requests in seconds.
pub const HF_REQUEST_TIMEOUT_SECS: u64 = 15;

/// Default number of deliveries per page.
pub const DELIVERIES_PER_PAGE: i64 = 20;

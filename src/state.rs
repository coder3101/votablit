use reqwest::Client;
use sqlx::SqlitePool;

/// Shared application state. All fields are cheap to clone (internally
/// `Arc`-wrapped), so Axum can pass this by value into every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub http_client: Client,
    pub admin_token: String,
    /// Whether to validate model IDs against the HuggingFace API.
    /// Set to `false` in tests to avoid network dependencies.
    pub validate_hf: bool,
}

impl AppState {
    /// Create a new `AppState` with HuggingFace validation enabled.
    pub fn new(db: SqlitePool, admin_token: String) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(crate::constants::HF_REQUEST_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            db,
            http_client,
            admin_token,
            validate_hf: true,
        }
    }

    /// Create a new `AppState` with HuggingFace validation disabled (for tests).
    pub fn new_without_hf_validation(db: SqlitePool, admin_token: String) -> Self {
        let mut state = Self::new(db, admin_token);
        state.validate_hf = false;
        state
    }
}

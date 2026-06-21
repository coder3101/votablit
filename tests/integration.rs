use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use votablit::{app, state::AppState};
use sqlx::SqlitePool;
use std::net::SocketAddr;

const TEST_ADMIN_TOKEN: &str = "test-secret-token";

/// Build a test app from a `#[sqlx::test]`-provided pool.
/// HF validation is disabled to avoid network dependencies in tests.
fn test_app(pool: SqlitePool) -> axum::Router {
    let state = AppState::new_without_hf_validation(pool, TEST_ADMIN_TOKEN.into());
    app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))))
}

/// Build a test app WITH HuggingFace validation enabled (for HF-specific tests).
fn test_app_with_hf(pool: SqlitePool) -> axum::Router {
    let state = AppState::new(pool, TEST_ADMIN_TOKEN.into());
    app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))))
}

/// Helper to send a request and return (status, body bytes).
async fn send(app: &mut axum::Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    use tower::ServiceExt;
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

fn json_post(path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_post_with_ip(path: &str, body: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("x-forwarded-for", ip)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_post_with_auth(path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TEST_ADMIN_TOKEN}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_put_with_auth(path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TEST_ADMIN_TOKEN}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json_delete_with_auth(path: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(path)
        .header("authorization", format!("Bearer {TEST_ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

// ===================================================================
// HTML page tests
// ===================================================================

#[sqlx::test]
async fn index_returns_200(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, body) = send(&mut app, get("/")).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("Abliteration"));
}

#[sqlx::test]
async fn leaderboard_table_returns_200(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, _) = send(&mut app, get("/leaderboard/table")).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn deliveries_list_returns_200(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, _) = send(&mut app, get("/deliveries/list")).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn model_select_returns_200(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, _) = send(&mut app, get("/model-select")).await;
    assert_eq!(status, StatusCode::OK);
}

// ===================================================================
// Leaderboard API tests
// ===================================================================

#[sqlx::test]
async fn get_leaderboard_empty(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, body) = send(&mut app, get("/api/leaderboard")).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!([]));
}

#[sqlx::test]
async fn get_deliveries_returns_seeded_data(pool: SqlitePool) {
    let mut app = test_app(pool);
    let (status, body) = send(&mut app, get("/api/deliveries")).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 57);
    assert_eq!(json["page"], 1);
    assert!(json["items"].as_array().unwrap().len() <= 20);
}

// ===================================================================
// Model creation tests
// ===================================================================

#[sqlx::test]
async fn create_model_via_admin(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "test/model-1", "hf_link": ""}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model_id"], "test/model-1");

    // Verify it appears in leaderboard
    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["model_id"], "test/model-1");
}

#[sqlx::test]
async fn create_model_duplicate_conflict(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "test/model-1"}"#,
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "test/model-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn create_model_empty_id_bad_request(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_post_with_auth("/api/admin/models", r#"{"model_id": ""}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("required"));
}

#[sqlx::test]
async fn create_model_unauthorized(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, _) = send(
        &mut app,
        json_post("/api/admin/models", r#"{"model_id": "test/model-1"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn create_model_wrong_token(pool: SqlitePool) {
    let mut app = test_app(pool);

    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/models")
        .header("content-type", "application/json")
        .header("authorization", "Bearer wrong-token")
        .body(Body::from(r#"{"model_id": "test/model-1"}"#))
        .unwrap();

    let (status, _) = send(&mut app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn create_model_via_user_form(pool: SqlitePool) {
    let mut app = test_app(pool);

    let req = Request::builder()
        .method("POST")
        .uri("/add-model")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("model_id=user/model-1"))
        .unwrap();

    let (status, body) = send(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("submitted for voting"));
}

#[sqlx::test]
async fn create_model_user_max_models(pool: SqlitePool) {
    let mut app = test_app(pool);

    for i in 0..50 {
        let body = format!(r#"{{"model_id": "test/model-{i}"}}"#);
        send(
            &mut app,
            json_post_with_auth("/api/admin/models", &body),
        )
        .await;
    }

    let (status, body) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "test/model-50"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("50"));
}

// ===================================================================
// Voting API tests
// ===================================================================

#[sqlx::test]
async fn vote_creates_model_and_records_vote(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "new-model"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["message"].as_str().unwrap().contains("successfully"));

    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["model_id"], "new-model");
    assert_eq!(json[0]["vote_count"], 1);
}

#[sqlx::test]
async fn vote_duplicate_rejected(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "model-a"}"#,
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "model-a"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn vote_empty_fields_bad_request(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, _) = send(
        &mut app,
        json_post("/api/vote", r#"{"client_uuid": "", "model_id": ""}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn vote_same_model_different_uuids_accepted(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "model-a"}"#,
            "10.0.0.1",
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "user-2", "model_id": "model-a"}"#,
            "10.0.0.2",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn vote_same_uuid_different_models_accepted(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "model-a"}"#,
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "user-1", "model_id": "model-b"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

// ===================================================================
// Admin update model tests
// ===================================================================

#[sqlx::test]
async fn admin_update_nonexistent_model(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_put_with_auth(
            "/api/admin/models/nonexistent",
            r#"{"hf_link": "https://huggingface.co/Qwen/Qwen2.5-7B"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

// ===================================================================
// Admin deliver tests
// ===================================================================

#[sqlx::test]
async fn admin_deliver_nonexistent_model(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/deliver",
            r#"{"model_id": "nonexistent", "hf_link": "https://hf.co/test", "kl_divergence": 0.02, "refused": 0, "total_prompts": 128}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

#[sqlx::test]
async fn admin_deliver_empty_fields(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, _) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/deliver",
            r#"{"model_id": "", "hf_link": "", "kl_divergence": 0.0, "refused": 0, "total_prompts": 1}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ===================================================================
// Deliver + deliveries list tests
// ===================================================================

#[sqlx::test]
async fn deliver_model_and_list_appears(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "deliver-me", "hf_link": ""}"#,
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post_with_auth(
            "/api/admin/deliver",
            r#"{"model_id": "deliver-me", "hf_link": "https://hf.co/test", "notes": "test note", "kl_divergence": 0.023, "refused": 1, "total_prompts": 128}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, body) = send(&mut app, get("/api/deliveries?per_page=100")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // 57 seeded + 1 new delivery
    assert_eq!(json["total"], 58);
    // Find our delivery in the items
    let items = json["items"].as_array().unwrap();
    let ours = items.iter().find(|i| i["model_id"] == "deliver-me").unwrap();
    assert_eq!(ours["hf_link"], "https://hf.co/test");
    assert_eq!(ours["notes"], "test note");
    assert_eq!(ours["kl_divergence"], 0.023);
    assert_eq!(ours["refused"], 1);
    assert_eq!(ours["total_prompts"], 128);
    assert!(
        !json["items"][0]["delivered_at"].as_str().unwrap().is_empty(),
        "delivered_at should be populated"
    );

    // Model should be removed from leaderboard after delivery
    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0, "delivered model should be removed from leaderboard");
}

// ===================================================================
// Form-based vote page tests
// ===================================================================

#[sqlx::test]
async fn vote_page_creates_model_and_vote(pool: SqlitePool) {
    let mut app = test_app(pool);

    let req = Request::builder()
        .method("POST")
        .uri("/vote")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("client_uuid=user-1&model_id=page-model"))
        .unwrap();

    let (status, body) = send(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("Vote recorded successfully"));

    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["model_id"], "page-model");
    assert_eq!(json[0]["vote_count"], 1);
}

#[sqlx::test]
async fn vote_page_duplicate_rejected(pool: SqlitePool) {
    let mut app = test_app(pool);

    let req = Request::builder()
        .method("POST")
        .uri("/vote")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("client_uuid=user-1&model_id=model-a"))
        .unwrap();
    send(&mut app, req).await;

    let req = Request::builder()
        .method("POST")
        .uri("/vote")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("client_uuid=user-1&model_id=model-a"))
        .unwrap();
    let (status, body) = send(&mut app, req).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("already voted"));
}

// ===================================================================
// Rate limiting tests
// ===================================================================

#[sqlx::test]
async fn vote_rate_limit_max_3_per_ip_per_hour(pool: SqlitePool) {
    let mut app = test_app(pool);

    for i in 1..=3 {
        let body = format!(r#"{{"client_uuid": "u{i}", "model_id": "rate-model-{i}"}}"#);
        let (status, _) = send(&mut app, json_post_with_ip("/api/vote", &body, "10.0.1.1")).await;
        assert_eq!(status, StatusCode::CREATED, "vote {i} should succeed");
    }

    let body = r#"{"client_uuid": "u4", "model_id": "rate-model-4"}"#;
    let (status, _) = send(&mut app, json_post_with_ip("/api/vote", body, "10.0.1.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn vote_same_ip_same_model_rejected(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "u1", "model_id": "m1"}"#,
            "10.0.2.1",
        ),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "u2", "model_id": "m1"}"#,
            "10.0.2.1",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

// ===================================================================
// Model creation via JSON API tests
// ===================================================================

#[sqlx::test]
async fn create_model_via_json_api(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(
        &mut app,
        json_post(
            "/api/models",
            r#"{"model_id": "json/model-1"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model_id"], "json/model-1");
}

#[sqlx::test]
async fn create_model_json_api_empty_id(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, _) = send(
        &mut app,
        json_post("/api/models", r#"{"model_id": ""}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn create_model_json_api_duplicate(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post("/api/models", r#"{"model_id": "dup-model"}"#),
    )
    .await;

    let (status, _) = send(
        &mut app,
        json_post("/api/models", r#"{"model_id": "dup-model"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// ===================================================================
// Admin update model tests
// ===================================================================

#[sqlx::test]
async fn admin_update_model_sets_hf_link(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "update-me"}"#,
        ),
    )
    .await;

    let (status, body) = send(
        &mut app,
        json_put_with_auth(
            "/api/admin/models/update-me",
            r#"{"hf_link": "https://huggingface.co/meta-llama/Llama-3.1-8B"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model_id"], "update-me");

    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json[0]["hf_link"].as_str().unwrap(),
        "https://huggingface.co/meta-llama/Llama-3.1-8B"
    );
}

#[sqlx::test]
async fn admin_update_model_empty_hf_link_rejected(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "noop-update"}"#,
        ),
    )
    .await;

    let (status, body) = send(
        &mut app,
        json_put_with_auth(
            "/api/admin/models/noop-update",
            r#"{"hf_link": ""}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("empty"));
}

// ===================================================================
// Edge cases
// ===================================================================

#[sqlx::test]
async fn get_leaderboard_returns_descending_order(pool: SqlitePool) {
    let mut app = test_app(pool);

    send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "u1", "model_id": "model-a"}"#,
            "10.0.0.1",
        ),
    )
    .await;
    send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "u2", "model_id": "model-a"}"#,
            "10.0.0.2",
        ),
    )
    .await;
    send(
        &mut app,
        json_post_with_ip(
            "/api/vote",
            r#"{"client_uuid": "u3", "model_id": "model-b"}"#,
            "10.0.0.3",
        ),
    )
    .await;

    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["model_id"], "model-a");
    assert_eq!(json[0]["vote_count"], 2);
    assert_eq!(json[1]["model_id"], "model-b");
    assert_eq!(json[1]["vote_count"], 1);
}

// ===================================================================
// Admin delete model tests
// ===================================================================

#[sqlx::test]
async fn admin_delete_model_removes_model_and_votes(pool: SqlitePool) {
    let mut app = test_app(pool);

    // Create a model and add a vote
    send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "u1", "model_id": "delete-me"}"#,
        ),
    )
    .await;

    // Verify it exists
    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);

    // Delete it
    let (status, body) = send(&mut app, json_delete_with_auth("/api/admin/models/delete-me")).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["model_id"], "delete-me");

    // Verify it's gone
    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn admin_delete_nonexistent_model(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(&mut app, json_delete_with_auth("/api/admin/models/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

// ===================================================================
// Admin prune models tests
// ===================================================================

#[sqlx::test]
async fn admin_prune_removes_zero_vote_models(pool: SqlitePool) {
    let mut app = test_app(pool);

    // Create models via admin (0 votes each)
    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "zero-a"}"#,
        ),
    )
    .await;
    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "zero-b"}"#,
        ),
    )
    .await;

    // Create one with a vote
    send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "u1", "model_id": "voted-model"}"#,
        ),
    )
    .await;

    // Prune
    let (status, body) = send(&mut app, json_delete_with_auth("/api/admin/prune")).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deleted"], 2);

    // Verify only the voted model remains
    let (_, body) = send(&mut app, get("/api/leaderboard")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["model_id"], "voted-model");
}

#[sqlx::test]
async fn admin_prune_when_no_zero_vote_models(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(&mut app, json_delete_with_auth("/api/admin/prune")).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deleted"], 0);
}

// ===================================================================
// Admin delete delivery tests
// ===================================================================

#[sqlx::test]
async fn admin_delete_delivery_removes_delivery(pool: SqlitePool) {
    let mut app = test_app(pool);

    // Create a model, then deliver it
    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/models",
            r#"{"model_id": "del-delivery-test", "hf_link": ""}"#,
        ),
    )
    .await;

    send(
        &mut app,
        json_post_with_auth(
            "/api/admin/deliver",
            r#"{"model_id": "del-delivery-test", "hf_link": "https://hf.co/test", "kl_divergence": 0.01, "refused": 0, "total_prompts": 100}"#,
        ),
    )
    .await;

    // Find the delivery ID
    let (_, body) = send(&mut app, get("/api/deliveries?per_page=100")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let initial_total = json["total"].as_i64().unwrap();
    let items = json["items"].as_array().unwrap();
    let ours = items.iter().find(|i| i["model_id"] == "del-delivery-test").unwrap();
    let delivery_id = ours["id"].as_i64().unwrap();

    // Delete the delivery
    let path = format!("/api/admin/deliveries/{delivery_id}");
    let (status, body) = send(&mut app, json_delete_with_auth(&path)).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["delivery_id"], delivery_id);

    // Verify it's gone
    let (_, body) = send(&mut app, get("/api/deliveries?per_page=100")).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"].as_i64().unwrap(), initial_total - 1);
    let items = json["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["model_id"] != "del-delivery-test"));
}

#[sqlx::test]
async fn admin_delete_delivery_nonexistent_returns_404(pool: SqlitePool) {
    let mut app = test_app(pool);

    let (status, body) = send(&mut app, json_delete_with_auth("/api/admin/deliveries/999999")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

#[sqlx::test]
async fn admin_delete_delivery_unauthorized(pool: SqlitePool) {
    let mut app = test_app(pool);

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/admin/deliveries/1")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&mut app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ===================================================================
// HuggingFace validation tests (use test_app_with_hf for real HF checks)
// ===================================================================

#[sqlx::test]
async fn submit_nonexistent_hf_model_rejected(pool: SqlitePool) {
    let mut app = test_app_with_hf(pool);

    // foo/bar does not exist on HuggingFace
    let (status, body) = send(
        &mut app,
        json_post("/api/models", r#"{"model_id": "foo/bar"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("not found"),
        "Expected 'not found' error, got: {}",
        json["error"]
    );
}

#[sqlx::test]
async fn vote_for_nonexistent_hf_model_rejected(pool: SqlitePool) {
    let mut app = test_app_with_hf(pool);

    let (status, body) = send(
        &mut app,
        json_post(
            "/api/vote",
            r#"{"client_uuid": "u1", "model_id": "foo/bar"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("not found"),
        "Expected 'not found' error, got: {}",
        json["error"]
    );
}

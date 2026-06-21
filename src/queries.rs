use sqlx::sqlite::Sqlite;

use crate::error::AppError;
use crate::models::{DeliveryEntry, DeliveryRow, LeaderboardEntry, ModelRow};

// ---------------------------------------------------------------------------
// Public query functions
//
// For SELECT queries returning rows we use `query_as!` with row structs
// (ModelRow, DeliveryRow). The macro maps columns to struct fields by name
// and checks types at compile time.
//
// For existence/count checks we use `query_scalar!`.
// For INSERT/UPDATE/DELETE we use `query!`.
// ---------------------------------------------------------------------------

/// Fetch all models ordered by vote count (descending).
pub async fn fetch_leaderboard(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
) -> Result<Vec<LeaderboardEntry>, AppError> {
    let rows = sqlx::query_as!(
        ModelRow,
        r#"SELECT model_id as "model_id!", vote_count, hf_link
           FROM models ORDER BY vote_count DESC"#
    )
    .fetch_all(exec)
    .await?;

    Ok(rows.into_iter().map(LeaderboardEntry::from).collect())
}

/// Sort order for deliveries.
#[derive(Debug, Clone, Copy, Default)]
pub enum DeliverySort {
    #[default]
    Date,
    KlDivergence,
    Refusal,
}

/// Fetch deliveries with pagination and sorting.
pub async fn fetch_deliveries(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    limit: i64,
    offset: i64,
    sort: DeliverySort,
) -> Result<Vec<DeliveryEntry>, AppError> {
    let rows = match sort {
        DeliverySort::Date => {
            sqlx::query_as!(
                DeliveryRow,
                r#"SELECT id, model_id as "model_id!", vote_count,
                          hf_link as "hf_link!", notes,
                          kl_divergence, refused, total_prompts,
                          delivered_at as "delivered_at!"
                   FROM deliveries ORDER BY delivered_at DESC LIMIT ? OFFSET ?"#,
                limit,
                offset
            )
            .fetch_all(exec)
            .await?
        }
        DeliverySort::KlDivergence => {
            sqlx::query_as!(
                DeliveryRow,
                r#"SELECT id, model_id as "model_id!", vote_count,
                          hf_link as "hf_link!", notes,
                          kl_divergence, refused, total_prompts,
                          delivered_at as "delivered_at!"
                   FROM deliveries ORDER BY kl_divergence ASC LIMIT ? OFFSET ?"#,
                limit,
                offset
            )
            .fetch_all(exec)
            .await?
        }
        DeliverySort::Refusal => {
            sqlx::query_as!(
                DeliveryRow,
                r#"SELECT id, model_id as "model_id!", vote_count,
                          hf_link as "hf_link!", notes,
                          kl_divergence, refused, total_prompts,
                          delivered_at as "delivered_at!"
                   FROM deliveries ORDER BY (CAST(refused AS REAL) / MAX(total_prompts, 1)) ASC LIMIT ? OFFSET ?"#,
                limit,
                offset
            )
            .fetch_all(exec)
            .await?
        }
    };

    Ok(rows.into_iter().map(DeliveryEntry::from).collect())
}

/// Count total number of deliveries.
pub async fn count_deliveries(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
) -> Result<i64, AppError> {
    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM deliveries")
        .fetch_one(exec)
        .await?;
    Ok(count)
}

/// Get the total number of models in the leaderboard.
pub async fn count_models(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
) -> Result<i64, AppError> {
    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM models")
        .fetch_one(exec)
        .await?;
    Ok(count)
}

/// Check whether a model has already been abliterated (case-insensitive).
pub async fn has_been_delivered(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
) -> Result<bool, AppError> {
    let lower = model_id.to_lowercase();
    let exists = sqlx::query_scalar!(
        "SELECT 1 FROM deliveries WHERE LOWER(model_id) = ?",
        lower
    )
    .fetch_optional(exec)
    .await?;
    Ok(exists.is_some())
}

/// Check whether a model with the given ID already exists (case-insensitive).
pub async fn model_exists(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
) -> Result<bool, AppError> {
    let lower = model_id.to_lowercase();
    let exists =
        sqlx::query_scalar!("SELECT 1 FROM models WHERE LOWER(model_id) = ?", lower)
            .fetch_optional(exec)
            .await?;
    Ok(exists.is_some())
}

/// Check whether the given IP created a model within the last day.
pub async fn has_recent_model_creation(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    ip: &str,
) -> Result<bool, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT 1 FROM models WHERE created_by_ip = ? AND created_at > datetime('now', '-1 day')",
        ip
    )
    .fetch_optional(exec)
    .await?;
    Ok(exists.is_some())
}

/// Check whether a client has already voted for a model.
pub async fn has_voted(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    client_uuid: &str,
    model_id: &str,
) -> Result<bool, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT 1 FROM votes WHERE client_uuid = ? AND model_id = ?",
        client_uuid,
        model_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(exists.is_some())
}

/// Check whether the given IP already voted for a model in the last hour.
pub async fn has_ip_voted_for_model(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    ip: &str,
    model_id: &str,
) -> Result<bool, AppError> {
    let exists = sqlx::query_scalar!(
        "SELECT 1 FROM votes WHERE ip_address = ? AND model_id = ? \
         AND voted_at > datetime('now', '-1 hour')",
        ip,
        model_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(exists.is_some())
}

/// Count how many times the given IP has voted in the last hour.
pub async fn count_ip_votes(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    ip: &str,
) -> Result<i64, AppError> {
    let count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM votes WHERE ip_address = ? \
         AND voted_at > datetime('now', '-1 hour')",
        ip
    )
    .fetch_one(exec)
    .await?;
    Ok(count)
}

/// Create a new model with no HF link.
pub async fn create_model(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
    ip: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO models (model_id, vote_count, created_by_ip) VALUES (?, 0, ?)",
        model_id,
        ip
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Create a new model with an HF link (admin only).
pub async fn create_model_with_hf(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
    hf_link: &Option<String>,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO models (model_id, vote_count, hf_link) VALUES (?, 0, ?)",
        model_id,
        hf_link
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Insert a vote record and increment the model's vote count.
///
/// Must be called inside a transaction.
pub async fn record_vote(
    tx: &mut sqlx::SqliteConnection,
    client_uuid: &str,
    model_id: &str,
    ip: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO votes (client_uuid, model_id, ip_address) VALUES (?, ?, ?)",
        client_uuid,
        model_id,
        ip
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE models SET vote_count = vote_count + 1 WHERE model_id = ?",
        model_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(())
}

/// Update the HF link for a model.
pub async fn update_model_hf_link(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    hf_link: &str,
    model_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE models SET hf_link = ? WHERE model_id = ?",
        hf_link,
        model_id
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Look up a model for delivery (returns model_id and vote_count).
pub async fn find_model_for_delivery(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
) -> Result<Option<(String, i64)>, AppError> {
    let row = sqlx::query!(
        r#"SELECT model_id as "model_id!", vote_count
           FROM models WHERE model_id = ?"#,
        model_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(row.map(|r| (r.model_id, r.vote_count)))
}

/// Insert a delivery record.
#[allow(clippy::too_many_arguments)]
pub async fn insert_delivery(
    exec: impl sqlx::Executor<'_, Database = Sqlite>,
    model_id: &str,
    vote_count: i64,
    hf_link: &str,
    notes: &Option<String>,
    kl_divergence: f64,
    refused: i64,
    total_prompts: i64,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO deliveries (model_id, vote_count, hf_link, notes, kl_divergence, refused, total_prompts) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        model_id,
        vote_count,
        hf_link,
        notes,
        kl_divergence,
        refused,
        total_prompts
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Delete a model by ID. Associated votes are removed automatically
/// via the `ON DELETE CASCADE` foreign key constraint.
///
/// Returns `true` if the model existed and was deleted.
pub async fn delete_model(
    pool: &sqlx::SqlitePool,
    model_id: &str,
) -> Result<bool, AppError> {
    let result = sqlx::query!("DELETE FROM models WHERE model_id = ?", model_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete all models with zero votes and return how many were deleted.
/// Associated votes (if any) are removed via `ON DELETE CASCADE`.
pub async fn prune_zero_vote_models(
    pool: &sqlx::SqlitePool,
) -> Result<i64, AppError> {
    let result = sqlx::query!(
        "DELETE FROM models WHERE vote_count = 0"
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() as i64)
}

/// Delete a delivery by ID.
///
/// Returns `true` if the delivery existed and was deleted.
pub async fn delete_delivery(
    pool: &sqlx::SqlitePool,
    delivery_id: i64,
) -> Result<bool, AppError> {
    let result = sqlx::query!("DELETE FROM deliveries WHERE id = ?", delivery_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn count_models_empty(pool: SqlitePool) {
        assert_eq!(count_models(&pool).await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn count_models_after_insert(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        assert_eq!(count_models(&pool).await.unwrap(), 1);
    }

    #[sqlx::test]
    async fn model_exists_false(pool: SqlitePool) {
        assert!(!model_exists(&pool, "nope").await.unwrap());
    }

    #[sqlx::test]
    async fn model_exists_true(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        assert!(model_exists(&pool, "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn create_model_inserts_row(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        let entry = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entry.len(), 1);
        assert_eq!(entry[0].model_id, "m1");
        assert_eq!(entry[0].vote_count, 0);
    }

    #[sqlx::test]
    async fn create_model_with_hf_stores_link(pool: SqlitePool) {
        create_model_with_hf(&pool, "m1", &Some("https://hf.co/test".into()))
            .await
            .unwrap();
        let entry = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entry[0].hf_link.as_deref(), Some("https://hf.co/test"));
    }

    #[sqlx::test]
    async fn has_recent_model_creation_false_empty(pool: SqlitePool) {
        assert!(!has_recent_model_creation(&pool, "1.2.3.4").await.unwrap());
    }

    #[sqlx::test]
    async fn has_recent_model_creation_true(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        assert!(has_recent_model_creation(&pool, "1.2.3.4").await.unwrap());
    }

    #[sqlx::test]
    async fn has_recent_model_creation_different_ip(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        assert!(!has_recent_model_creation(&pool, "5.6.7.8").await.unwrap());
    }

    #[sqlx::test]
    async fn has_voted_false(pool: SqlitePool) {
        assert!(!has_voted(&pool, "u1", "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn has_voted_true(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        assert!(has_voted(&pool, "u1", "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn has_ip_voted_for_model_false(pool: SqlitePool) {
        assert!(!has_ip_voted_for_model(&pool, "1.2.3.4", "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn has_ip_voted_for_model_true(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        assert!(has_ip_voted_for_model(&pool, "1.2.3.4", "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn count_ip_votes_zero(pool: SqlitePool) {
        assert_eq!(count_ip_votes(&pool, "1.2.3.4").await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn count_ip_votes_after_voting(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        create_model(&mut *tx, "m2", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u2", "m2", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(count_ip_votes(&pool, "1.2.3.4").await.unwrap(), 2);
    }

    #[sqlx::test]
    async fn record_vote_increments_count(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        let entry = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entry[0].vote_count, 1);
    }

    #[sqlx::test]
    async fn record_vote_multiple_increments(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u2", "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u3", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        let entry = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entry[0].vote_count, 3);
    }

    #[sqlx::test]
    async fn update_model_hf_link_sets_value(pool: SqlitePool) {
        create_model(&pool, "m1", "1.2.3.4").await.unwrap();
        update_model_hf_link(&pool, "https://hf.co/updated", "m1")
            .await
            .unwrap();
        let entry = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entry[0].hf_link.as_deref(), Some("https://hf.co/updated"));
    }

    #[sqlx::test]
    async fn find_model_for_delivery_none(pool: SqlitePool) {
        assert!(find_model_for_delivery(&pool, "nope").await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn find_model_for_delivery_some(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u2", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        let (id, votes) = find_model_for_delivery(&pool, "m1").await.unwrap().unwrap();
        assert_eq!(id, "m1");
        assert_eq!(votes, 2);
    }

    #[sqlx::test]
    async fn insert_delivery_and_fetch(pool: SqlitePool) {
        let seeded_count = count_deliveries(&pool).await.unwrap();

        insert_delivery(
            &pool, "test/inserted-model", 5, "https://hf.co/delivered", &Some("notes".into()),
            0.023, 1, 128,
        )
        .await
        .unwrap();

        // Total count should have increased by 1
        let new_count = count_deliveries(&pool).await.unwrap();
        assert_eq!(new_count, seeded_count + 1);

        // Fetch all and find our inserted row
        let deliveries = fetch_deliveries(&pool, 100, 0, DeliverySort::Date).await.unwrap();
        let ours = deliveries.iter().find(|d| d.model_id == "test/inserted-model").unwrap();
        assert_eq!(ours.vote_count, 5);
        assert_eq!(ours.hf_link, "https://hf.co/delivered");
        assert_eq!(ours.notes.as_deref(), Some("notes"));
        assert!((ours.kl_divergence - 0.023).abs() < f64::EPSILON);
        assert_eq!(ours.refused, 1);
        assert_eq!(ours.total_prompts, 128);
        assert!(
            !ours.delivered_at.is_empty(),
            "delivered_at should be populated, got empty string"
        );
    }

    #[sqlx::test]
    async fn fetch_leaderboard_descending_order(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        create_model(&mut *tx, "m2", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u2", "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u3", "m2", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();
        let entries = fetch_leaderboard(&pool).await.unwrap();
        assert_eq!(entries[0].model_id, "m1");
        assert_eq!(entries[0].vote_count, 2);
        assert_eq!(entries[1].model_id, "m2");
        assert_eq!(entries[1].vote_count, 1);
    }

    #[sqlx::test]
    async fn delete_model_removes_model_and_votes(pool: SqlitePool) {
        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "m1", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "m1", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();

        let deleted = delete_model(&pool, "m1").await.unwrap();
        assert!(deleted);
        assert_eq!(count_models(&pool).await.unwrap(), 0);
        assert!(!model_exists(&pool, "m1").await.unwrap());
    }

    #[sqlx::test]
    async fn delete_nonexistent_model_returns_false(pool: SqlitePool) {
        let deleted = delete_model(&pool, "nope").await.unwrap();
        assert!(!deleted);
    }

    #[sqlx::test]
    async fn prune_zero_vote_models_removes_only_zero_vote(pool: SqlitePool) {
        create_model(&pool, "zero", "1.2.3.4").await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        create_model(&mut *tx, "voted", "1.2.3.4").await.unwrap();
        record_vote(&mut tx, "u1", "voted", "1.2.3.4").await.unwrap();
        tx.commit().await.unwrap();

        let pruned = prune_zero_vote_models(&pool).await.unwrap();
        assert_eq!(pruned, 1);
        assert!(!model_exists(&pool, "zero").await.unwrap());
        assert!(model_exists(&pool, "voted").await.unwrap());
    }

    #[sqlx::test]
    async fn prune_when_no_zero_vote_models(pool: SqlitePool) {
        let pruned = prune_zero_vote_models(&pool).await.unwrap();
        assert_eq!(pruned, 0);
    }

    #[sqlx::test]
    async fn delete_delivery_removes_row(pool: SqlitePool) {
        insert_delivery(
            &pool, "test/del-delivery", 5, "https://hf.co/test", &None,
            0.02, 1, 128,
        )
        .await
        .unwrap();

        let deliveries = fetch_deliveries(&pool, 100, 0, DeliverySort::Date).await.unwrap();
        let ours = deliveries.iter().find(|d| d.model_id == "test/del-delivery").unwrap();
        let id = ours.id;

        let deleted = delete_delivery(&pool, id).await.unwrap();
        assert!(deleted);

        // Verify it's gone
        let deliveries = fetch_deliveries(&pool, 100, 0, DeliverySort::Date).await.unwrap();
        assert!(deliveries.iter().all(|d| d.model_id != "test/del-delivery"));
    }

    #[sqlx::test]
    async fn delete_delivery_nonexistent_returns_false(pool: SqlitePool) {
        let deleted = delete_delivery(&pool, 999999).await.unwrap();
        assert!(!deleted);
    }
}

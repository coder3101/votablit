use sqlx::SqlitePool;
use tracing::info;

/// Run database migrations using the files in `migrations/`.
///
/// Uses `sqlx::migrate!()` for compile-time verification of migration files.
/// Safe to call multiple times — already-applied migrations are skipped.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    info!("Database initialized successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn init_db_creates_tables(pool: SqlitePool) {
        init_db(&pool).await.unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM models")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }

    #[sqlx::test]
    async fn init_db_is_idempotent(pool: SqlitePool) {
        init_db(&pool).await.unwrap();
        init_db(&pool).await.unwrap();
    }
}

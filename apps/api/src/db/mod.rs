use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    PgConnection,
};
use std::time::Duration;

pub async fn create_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await?;
    Ok(pool)
}

// ponytail: connect lazily where needed; migrate! in main is the real bootstrap path.
pub async fn ping(_conn: &mut PgConnection) -> sqlx::Result<()> {
    Ok(())
}

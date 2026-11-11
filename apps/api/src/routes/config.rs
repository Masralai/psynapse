use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/config", get(get_config))
}

async fn get_config(State(state): State<AppState>) -> Json<Value> {
    let db_status = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => "online",
        Err(_) => "offline",
    };
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "latestVersion": Value::Null,
        "hasUpdate": false,
        "dbStatus": db_status,
    }))
}

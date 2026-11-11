use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/auth/status", get(auth_status))
}

async fn auth_status(State(state): State<AppState>) -> Json<Value> {
    let auth_enabled = state.app_password.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
    Json(json!({
        "auth_enabled": auth_enabled,
        "message": if auth_enabled { "Authentication is required" } else { "Authentication is disabled" }
    }))
}

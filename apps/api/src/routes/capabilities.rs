use axum::{routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/capabilities", get(get_capabilities))
}

#[derive(Serialize)]
struct CapabilitiesResponse {
    docling_available: bool,
    crawl4ai_available: bool,
    crawl4ai_remote_configured: bool,
}

async fn get_capabilities() -> Json<CapabilitiesResponse> {
    // ponytail: heavy extraction runtimes not bundled in Rust port yet; report unavailable honestly.
    Json(CapabilitiesResponse {
        docling_available: false,
        crawl4ai_available: false,
        crawl4ai_remote_configured: false,
    })
}

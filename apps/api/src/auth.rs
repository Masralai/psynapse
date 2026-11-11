use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unauthorized(detail: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", "Bearer")],
        Json(json!({"detail": detail})),
    )
        .into_response()
}

pub async fn require_password(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let Some(password) = state.app_password.as_ref().filter(|p| !p.is_empty()) else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    // Exclude parity endpoints that must be reachable without auth (matches Open Notebook behavior subset).
    if matches!(
        path,
        "/health"
            | "/api/auth/status"
            | "/api/config"
            | "/"
            | "/docs"
            | "/openapi.json"
            | "/redoc"
    ) {
        return next.run(req).await;
    }
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let Some(auth_header) = auth_header else {
        return unauthorized("Missing authorization header");
    };

    let mut parts = auth_header.splitn(2, ' ');
    let scheme = parts.next().unwrap_or("");
    let credentials = parts.next().unwrap_or("");
    if !scheme.eq_ignore_ascii_case("bearer") {
        return unauthorized("Invalid authorization header format");
    }

    if !ct_eq(credentials.as_bytes(), password.as_bytes()) {
        return unauthorized("Invalid password");
    }

    next.run(req).await
}

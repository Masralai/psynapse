pub mod commands;
pub mod health;
pub mod notebooks;
pub mod sources;

use crate::state::AppState;
use axum::Router;

pub fn api_routes(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .nest(
            "/api",
            notebooks::routes()
                .merge(sources::routes())
                .merge(commands::routes()),
        )
        .with_state(state)
}

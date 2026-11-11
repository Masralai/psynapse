pub mod auth;
pub mod capabilities;
pub mod chat;
pub mod commands;
pub mod config;
pub mod credentials;
pub mod embedding;
pub mod episode_profiles;
pub mod health;
pub mod insights;
pub mod models;
pub mod notebooks;
pub mod notes;
pub mod podcasts;
pub mod providers;
pub mod search;
pub mod settings;
pub mod source_chat;
pub mod sources;
pub mod speaker_profiles;
pub mod transformations;

use crate::state::AppState;
use axum::Router;

pub fn api_routes(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .nest(
            "/api",
            auth::routes()
                .merge(notebooks::routes())
                .merge(sources::routes())
                .merge(commands::routes())
                .merge(notes::routes())
                .merge(settings::routes())
                .merge(config::routes())
                .merge(capabilities::routes())
                .merge(transformations::routes())
                .merge(search::routes())
                .merge(chat::routes())
                .merge(source_chat::routes())
                .merge(models::routes())
                .merge(credentials::routes())
                .merge(providers::routes())
                .merge(embedding::routes())
                .merge(insights::routes())
                .merge(episode_profiles::routes())
                .merge(speaker_profiles::routes())
                .merge(podcasts::routes()),
        )
        .with_state(state)
}

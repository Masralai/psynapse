use axum::{extract::DefaultBodyLimit, http::HeaderValue, middleware};
use psynapse_api::{auth, config::Config, db, routes, state::AppState};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "psynapse_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let pool = db::create_pool(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    tokio::spawn(psynapse_api::jobs::run_worker(
        pool.clone(),
        config.job_poll_interval_secs,
        config.job_retry_backoff_secs,
    ));

    let state = AppState {
        pool,
        app_password: config.app_password.clone(),
    };
    // ponytail: configurable CORS origins via env with permissive fallback; full CORS/auth policy parity pending.
    let cors = {
        let origins: Vec<HeaderValue> = config.cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        if origins.is_empty() {
            CorsLayer::permissive()
        } else {
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    };

    let app = routes::api_routes(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_password,
        ))
        .layer(DefaultBodyLimit::max(config.max_upload_size_bytes))
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}




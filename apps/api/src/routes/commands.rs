use crate::{errors::AppError, jobs, state::AppState};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/commands/jobs", get(list_command_jobs))
        .route(
            "/commands/jobs/:job_id",
            get(get_command_job_status).delete(cancel_command_job),
        )
        .route("/commands/registry/debug", get(debug_registry))
}

#[derive(Deserialize)]
pub struct ListParams {
    pub command_filter: Option<String>,
    pub status_filter: Option<String>,
    pub limit: Option<i64>,
}

async fn get_command_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&job_id).map_err(|_| AppError::NotFound)?;
    Ok(Json(jobs::status_payload(&state.pool, id).await?))
}

async fn list_command_jobs(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Value>>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query(
        r#"
        SELECT id, app_name, command_name, status, result, error_message, created_at, started_at, completed_at, input
        FROM jobs
        WHERE ($1::text IS NULL OR command_name = $1)
          AND ($2::text IS NULL OR status = $2)
        ORDER BY created_at DESC
        LIMIT $3
        "#,
    )
    .bind(params.command_filter)
    .bind(params.status_filter)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    let out = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid,_>("id").to_string(),
                "app_name": r.get::<String,_>("app_name"),
                "name": r.get::<String,_>("command_name"),
                "status": r.get::<String,_>("status"),
                "result": r.get::<Option<Value>,_>("result"),
                "error_message": r.get::<Option<String>,_>("error_message"),
                "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                "started_at": r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("started_at").map(|t| t.to_rfc3339()),
                "completed_at": r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at").map(|t| t.to_rfc3339()),
                "input": r.get::<Value,_>("input"),
            })
        })
        .collect();
    Ok(Json(out))
}

async fn cancel_command_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&job_id).map_err(|_| AppError::NotFound)?;
    let res = sqlx::query(
        "UPDATE jobs SET status='canceled', error_message=COALESCE(error_message,'Canceled by user'), completed_at=NOW() WHERE id=$1 AND status IN ('pending','queued','running')",
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        // ponytail: return current state shape-compatible enough for UI pollers instead of hard failing.
        let _ = jobs::status_payload(&state.pool, id).await?;
    }
    Ok(Json(serde_json::json!({"job_id": id.to_string(), "cancelled": res.rows_affected() > 0})))
}

async fn debug_registry() -> Json<Value> {
    // ponytail: static registry metadata; handler introspection/observability parity pending.
    let cmds = vec![
        ("process_source", "Extract/index source content and run requested post-processing"),
        ("run_transformation", "Generate and persist a source insight from a transformation"),
        ("embed_source", "Chunk and embed a source's text into pgvector storage"),
        ("embed_note", "Embed a note's content for vector search"),
        ("rebuild_embeddings", "Rebuild embeddings for selected content types"),
        ("generate_podcast", "Generate podcast outline/transcript artifacts for an episode (audio pending)"),
    ];
    let items: Vec<Value> = cmds
        .iter()
        .map(|(n, desc)| serde_json::json!({
            "app_id":"psynapse",
            "name":n,
            "full_id":format!("psynapse.{n}"),
            "description": desc,
            "has_handler": true
        }))
        .collect();
    let names: Vec<&str> = cmds.iter().map(|(n, _)| *n).collect();
    Json(serde_json::json!({
        "total_commands": cmds.len(),
        "commands_by_app": { "psynapse": names },
        "command_items": items
    }))
}
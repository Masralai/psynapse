use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, jobs, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/embed", post(embed_content))
        .route("/embeddings/rebuild", post(start_rebuild))
        .route("/embeddings/rebuild/:command_id/status", get(rebuild_status))
}

#[derive(Deserialize)]
pub struct EmbedRequest {
    pub item_id: String,
    pub item_type: String,
    #[serde(default)]
    pub async_processing: bool,
}

#[derive(Serialize)]
pub struct EmbedResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks_created: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
}

#[derive(Deserialize)]
pub struct RebuildRequest {
    pub mode: String,
    #[serde(default = "t")] pub include_sources: bool,
    #[serde(default = "t")] pub include_notes: bool,
    #[serde(default = "t")] pub include_insights: bool,
}
fn t() -> bool { true }

async fn embed_content(
    State(state): State<AppState>,
    Json(req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, AppError> {
    let item_type = req.item_type.to_lowercase();
    if item_type != "source" && item_type != "note" {
        return Err(AppError::BadRequest("Item type must be either 'source' or 'note'".into()));
    }
    let item_uuid = Uuid::parse_str(&req.item_id).map_err(|_| {
        AppError::Http(axum::http::StatusCode::NOT_FOUND, format!("{} not found", req.item_type))
    })?;
    if item_type == "source" {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM sources WHERE id=$1").bind(item_uuid).fetch_optional(&state.pool).await?;
        if exists.is_none() { return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "source not found".into())); }
    } else {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM notes WHERE id=$1").bind(item_uuid).fetch_optional(&state.pool).await?;
        if exists.is_none() { return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "note not found".into())); }
    }

    // ponytail: embedding provider not wired; enqueue job for worker to complete as placeholder.
    let cmd_name = if item_type == "source" { "embed_source" } else { "embed_note" };
    let input = json!({ if item_type == "source" { "source_id" } else { "note_id" }: req.item_id });
    let cid = jobs::enqueue(&state.pool, "psynapse", cmd_name, input).await?;

    let message = if req.async_processing {
        "Embedding queued for background processing".to_string()
    } else if item_type == "source" {
        "Source embedding job submitted".to_string()
    } else {
        "Note embedding job submitted".to_string()
    };
    Ok(Json(EmbedResponse {
        success: true,
        message,
        chunks_created: if req.async_processing { None } else { Some(0) },
        command_id: Some(cid.to_string()),
        item_id: Some(req.item_id),
        item_type: Some(item_type),
    }))
}

async fn start_rebuild(
    State(state): State<AppState>,
    Json(req): Json<RebuildRequest>,
) -> Result<Json<Value>, AppError> {
    let mut total: i64 = 0;
    if req.include_sources {
        total += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sources WHERE full_text IS NOT NULL AND btrim(full_text) <> ''")
            .fetch_one(&state.pool).await.unwrap_or(0);
    }
    if req.include_notes {
        total += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM notes WHERE content IS NOT NULL AND btrim(content) <> ''")
            .fetch_one(&state.pool).await.unwrap_or(0);
    }
    if req.include_insights {
        total += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM source_insights")
            .fetch_one(&state.pool).await.unwrap_or(0);
    }
    let input = json!({
        "mode": req.mode,
        "include_sources": req.include_sources,
        "include_notes": req.include_notes,
        "include_insights": req.include_insights,
        "total_items": total,
    });
    let cid = jobs::enqueue(&state.pool, "psynapse", "rebuild_embeddings", input).await?;
    Ok(Json(json!({
        "command_id": cid.to_string(),
        "total_items": total,
        "estimated_items": total,
        "message": format!("Rebuild operation started. Estimated {total} items to process."),
    })))
}

async fn rebuild_status(
    State(state): State<AppState>,
    Path(command_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&command_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query(
        "SELECT status, result, error_message, created_at, started_at, completed_at, input FROM jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let status: String = row.get("status");
    let result: Option<Value> = row.try_get("result").ok().flatten();
    let input: Option<Value> = row.try_get("input").ok().flatten();
    let total = result.as_ref().and_then(|r| r.get("total_items")).and_then(|v| v.as_i64())
        .or_else(|| input.as_ref().and_then(|i| i.get("total_items")).and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let processed = result.as_ref().and_then(|r| r.get("processed").or(r.get("jobs_submitted"))).and_then(|v| v.as_i64()).unwrap_or(0);
    let failed = result.as_ref().and_then(|r| r.get("failed").or(r.get("failed_items")).or(r.get("failed_submissions"))).and_then(|v| v.as_i64()).unwrap_or(0);
    let pct = if total > 0 { ((processed as f64 / total as f64) * 100.0 * 100.0).round() / 100.0 } else { 0.0 };
    Ok(Json(json!({
        "command_id": id.to_string(),
        "status": status,
        "progress": {
            "processed": processed, "total": total, "percentage": pct,
            "processed_items": processed, "total_items": total, "failed_items": failed
        },
        "stats": result.as_ref().and_then(|r| r.get("stats")).cloned().unwrap_or(json!({
            "sources": result.as_ref().and_then(|r| r.get("sources")).and_then(|v| v.as_i64()).unwrap_or(0),
            "notes": result.as_ref().and_then(|r| r.get("notes")).and_then(|v| v.as_i64()).unwrap_or(0),
            "insights": result.as_ref().and_then(|r| r.get("insights")).and_then(|v| v.as_i64()).unwrap_or(0),
            "failed": failed,
        })),
        "started_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at").ok().flatten().map(|t| t.to_rfc3339()),
        "completed_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at").ok().flatten().map(|t| t.to_rfc3339()),
        "error_message": row.try_get::<Option<String>, _>("error_message").ok().flatten(),
    })))
}

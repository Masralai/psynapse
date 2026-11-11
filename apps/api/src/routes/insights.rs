use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/insights/:insight_id",
            axum::routing::get(get_insight).delete(delete_insight),
        )
        .route("/insights/:insight_id/save-as-note", post(save_as_note))
}

#[derive(Serialize)]
pub struct SourceInsightResponse {
    pub id: String,
    pub source_id: String,
    pub insight_type: String,
    pub content: String,
    pub created: Option<String>,
    pub updated: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct SaveAsNoteRequest {
    pub notebook_id: Option<String>,
}

#[derive(Serialize)]
pub struct NoteResponse {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub note_type: Option<String>,
    pub created: String,
    pub updated: String,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }
fn fmt_opt(ts: Option<DateTime<Utc>>) -> Option<String> { ts.map(fmt) }

async fn get_insight(
    State(state): State<AppState>,
    Path(insight_id): Path<String>,
) -> Result<Json<SourceInsightResponse>, AppError> {
    let id = Uuid::parse_str(&insight_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query(
        "SELECT id, source_id, insight_type, content, created_at, updated_at FROM source_insights WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(SourceInsightResponse {
        id: row.get::<Uuid, _>("id").to_string(),
        source_id: row.get::<Uuid, _>("source_id").to_string(),
        insight_type: row.get("insight_type"),
        content: row.get("content"),
        created: fmt_opt(row.try_get("created_at").ok()),
        updated: fmt_opt(row.try_get("updated_at").ok()),
    }))
}

async fn delete_insight(
    State(state): State<AppState>,
    Path(insight_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&insight_id).map_err(|_| AppError::NotFound)?;
    let res = sqlx::query("DELETE FROM source_insights WHERE id=$1").bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"message": "Insight deleted successfully"})))
}

async fn save_as_note(
    State(state): State<AppState>,
    Path(insight_id): Path<String>,
    Json(req): Json<SaveAsNoteRequest>,
) -> Result<Json<NoteResponse>, AppError> {
    let id = Uuid::parse_str(&insight_id).map_err(|_| AppError::NotFound)?;
    let insight = sqlx::query("SELECT insight_type, content FROM source_insights WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let insight_type: String = insight.get("insight_type");
    let content: String = insight.get("content");
    let title = Some(format!("Insight: {}", insight_type.replace('_', " ")));

    let mut tx = state.pool.begin().await?;
    let note = sqlx::query(
        "INSERT INTO notes (title, content, note_type) VALUES ($1,$2,'ai') RETURNING id, title, content, note_type, created_at, updated_at",
    )
    .bind(&title)
    .bind(&content)
    .fetch_one(&mut *tx)
    .await?;
    let note_id: Uuid = note.get("id");

    if let Some(nb) = req.notebook_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM notebooks WHERE id=$1")
            .bind(nb).fetch_optional(&mut *tx).await?;
        if exists.is_none() {
            return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "Notebook not found".into()));
        }
        sqlx::query("INSERT INTO note_notebooks (note_id, notebook_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(note_id).bind(nb).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(Json(NoteResponse {
        id: note_id.to_string(),
        title: note.try_get("title").ok().flatten(),
        content: note.try_get("content").ok().flatten(),
        note_type: note.try_get("note_type").ok().flatten(),
        created: fmt(note.get("created_at")),
        updated: fmt(note.get("updated_at")),
    }))
}

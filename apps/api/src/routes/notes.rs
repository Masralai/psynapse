use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/:note_id", get(get_note).put(update_note).delete(delete_note))
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

#[derive(Deserialize)]
pub struct CreateNoteRequest {
    pub title: Option<String>,
    pub content: String,
    pub note_type: Option<String>,
    pub notebook_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub note_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub notebook_id: Option<String>,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }

fn to_resp(r: &sqlx::postgres::PgRow) -> NoteResponse {
    NoteResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        title: r.try_get("title").ok().flatten(),
        content: r.try_get("content").ok().flatten(),
        note_type: r.try_get("note_type").ok().flatten(),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
    }
}

async fn list_notes(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<NoteResponse>>, AppError> {
    let rows = if let Some(nb) = params.notebook_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        sqlx::query(
            r#"
            SELECT n.id, n.title, n.content, n.note_type, n.created_at, n.updated_at
            FROM notes n
            JOIN note_notebooks nn ON nn.note_id = n.id
            WHERE nn.notebook_id = $1
            ORDER BY n.updated_at DESC
            "#,
        )
        .bind(nb)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, title, content, note_type, created_at, updated_at FROM notes ORDER BY updated_at DESC",
        )
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(rows.iter().map(to_resp).collect()))
}

async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNoteRequest>,
) -> Result<Json<NoteResponse>, AppError> {
    let note_type = payload.note_type.unwrap_or_else(|| "human".into());
    if note_type != "human" && note_type != "ai" {
        return Err(AppError::BadRequest("note_type must be 'human' or 'ai'".into()));
    }
    let title = payload.title.or_else(|| {
        if note_type == "ai" {
            Some(payload.content.chars().take(60).collect::<String>())
        } else { None }
    });
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO notes (title, content, note_type) VALUES ($1,$2,$3) RETURNING id, title, content, note_type, created_at, updated_at",
    )
    .bind(title)
    .bind(&payload.content)
    .bind(&note_type)
    .fetch_one(&mut *tx)
    .await?;
    let note_id: Uuid = row.get("id");
    if let Some(nb) = payload.notebook_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        let exists = sqlx::query("SELECT 1 FROM notebooks WHERE id=$1").bind(nb).fetch_optional(&mut *tx).await?;
        if exists.is_none() { return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "Notebook not found".into())); }
        sqlx::query("INSERT INTO note_notebooks (note_id, notebook_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(note_id).bind(nb).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(to_resp(&row)))
}

async fn get_note(
    State(state): State<AppState>,
    Path(note_id): Path<String>,
) -> Result<Json<NoteResponse>, AppError> {
    let id = Uuid::parse_str(&note_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT id, title, content, note_type, created_at, updated_at FROM notes WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_resp(&row)))
}

async fn update_note(
    State(state): State<AppState>,
    Path(note_id): Path<String>,
    Json(payload): Json<UpdateNoteRequest>,
) -> Result<Json<NoteResponse>, AppError> {
    let id = Uuid::parse_str(&note_id).map_err(|_| AppError::NotFound)?;
    let cur = sqlx::query("SELECT title, content, note_type FROM notes WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let title = payload.title.or_else(|| cur.try_get("title").ok().flatten());
    let content = payload.content.or_else(|| cur.try_get("content").ok().flatten());
    let note_type = payload.note_type.or_else(|| cur.try_get("note_type").ok().flatten()).unwrap_or_else(|| "human".into());
    if note_type != "human" && note_type != "ai" {
        return Err(AppError::BadRequest("note_type must be 'human' or 'ai'".into()));
    }
    let row = sqlx::query(
        "UPDATE notes SET title=$1, content=$2, note_type=$3, updated_at=NOW() WHERE id=$4 RETURNING id, title, content, note_type, created_at, updated_at",
    )
    .bind(title).bind(content).bind(note_type).bind(id)
    .fetch_one(&state.pool).await?;
    Ok(Json(to_resp(&row)))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(note_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let id = Uuid::parse_str(&note_id).map_err(|_| AppError::NotFound)?;
    let res = sqlx::query("DELETE FROM notes WHERE id=$1").bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"message": "Note deleted successfully"})))
}

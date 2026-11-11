use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sources/:source_id/chat/sessions", get(list_sessions).post(create_session))
        .route(
            "/sources/:source_id/chat/sessions/:session_id",
            get(get_session).put(update_session).delete(delete_session),
        )
        .route(
            "/sources/:source_id/chat/sessions/:session_id/messages",
            post(send_message),
        )
}

#[derive(Serialize)]
pub struct ChatSessionResponse {
    pub id: String,
    pub title: String,
    pub source_id: String,
    pub model_override: Option<String>,
    pub created: String,
    pub updated: String,
    pub message_count: Option<i64>,
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: String,
    pub timestamp: Option<String>,
}

#[derive(Serialize)]
pub struct ContextIndicator {
    pub sources: Vec<String>,
    pub insights: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Serialize)]
pub struct ChatSessionWithMessagesResponse {
    pub id: String,
    pub title: String,
    pub source_id: String,
    pub model_override: Option<String>,
    pub created: String,
    pub updated: String,
    pub message_count: Option<i64>,
    pub messages: Vec<ChatMessage>,
    pub context_indicators: Option<ContextIndicator>,
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub source_id: Option<String>,
    pub title: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub message: String,
    pub model_override: Option<String>,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }

async fn ensure_source(pool: &sqlx::PgPool, source_id: &str) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(source_id).map_err(|_| AppError::Http(axum::http::StatusCode::NOT_FOUND, "Source not found".into()))?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM sources WHERE id=$1").bind(id).fetch_optional(pool).await?;
    if exists.is_none() { return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "Source not found".into())); }
    Ok(id)
}

async fn ensure_related(pool: &sqlx::PgPool, source_id: Uuid, session_id: Uuid) -> Result<(), AppError> {
    let ok = sqlx::query_scalar::<_, i64>("SELECT 1 FROM chat_session_sources WHERE session_id=$1 AND source_id=$2")
        .bind(session_id).bind(source_id).fetch_optional(pool).await?;
    if ok.is_none() { return Err(AppError::NotFound); }
    Ok(())
}

async fn get_messages(pool: &sqlx::PgPool, session_id: Uuid) -> Result<Vec<ChatMessage>, AppError> {
    let rows = sqlx::query(
        "SELECT id, role, content, created_at FROM chat_messages WHERE session_id=$1 ORDER BY created_at ASC",
    ).bind(session_id).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| ChatMessage {
        id: r.get::<Uuid,_>("id").to_string(),
        msg_type: r.get::<String,_>("role"),
        content: r.get::<String,_>("content"),
        timestamp: Some(fmt(r.get("created_at"))),
    }).collect())
}

async fn list_sessions(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Vec<ChatSessionResponse>>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.title, s.model_override, s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM chat_messages m WHERE m.session_id = s.id) as message_count
        FROM chat_sessions s
        JOIN chat_session_sources css ON css.session_id = s.id
        WHERE css.source_id = $1
        ORDER BY s.updated_at DESC
        "#,
    ).bind(sid).fetch_all(&state.pool).await?;
    Ok(Json(rows.into_iter().map(|r| ChatSessionResponse {
        id: r.get::<Uuid,_>("id").to_string(),
        title: r.get::<Option<String>,_>("title").unwrap_or_else(|| "Untitled Session".into()),
        source_id: sid.to_string(),
        model_override: r.try_get("model_override").ok().flatten(),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
        message_count: Some(r.get::<i64,_>("message_count")),
    }).collect()))
}

async fn create_session(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(p): Json<CreateSessionRequest>,
) -> Result<Json<ChatSessionResponse>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let title = p.title.unwrap_or_else(|| "Source Chat".into());
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO chat_sessions (title, model_override) VALUES ($1,$2) RETURNING id, title, model_override, created_at, updated_at",
    ).bind(&title).bind(&p.model_override).fetch_one(&mut *tx).await?;
    let sess_id: Uuid = row.get("id");
    sqlx::query("INSERT INTO chat_session_sources (session_id, source_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
        .bind(sess_id).bind(sid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(ChatSessionResponse {
        id: sess_id.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or(title),
        source_id: sid.to_string(),
        model_override: row.try_get("model_override").ok().flatten(),
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(0),
    }))
}

async fn get_session(
    State(state): State<AppState>,
    Path((source_id, session_id)): Path<(String, String)>,
) -> Result<Json<ChatSessionWithMessagesResponse>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let sess = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    ensure_related(&state.pool, sid, sess).await?;
    let row = sqlx::query("SELECT id, title, model_override, created_at, updated_at FROM chat_sessions WHERE id=$1")
        .bind(sess).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let messages = get_messages(&state.pool, sess).await?;
    Ok(Json(ChatSessionWithMessagesResponse {
        id: sess.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or_else(|| "Untitled Session".into()),
        source_id: sid.to_string(),
        model_override: row.try_get("model_override").ok().flatten(),
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(messages.len() as i64),
        messages,
        context_indicators: None,
    }))
}

async fn update_session(
    State(state): State<AppState>,
    Path((source_id, session_id)): Path<(String, String)>,
    Json(p): Json<UpdateSessionRequest>,
) -> Result<Json<ChatSessionResponse>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let sess = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    ensure_related(&state.pool, sid, sess).await?;
    let cur = sqlx::query("SELECT title, model_override FROM chat_sessions WHERE id=$1").bind(sess).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let title = p.title.or_else(|| cur.try_get("title").ok().flatten()).unwrap_or_else(|| "Untitled Session".into());
    let model_override = if p.model_override.is_some() { p.model_override } else { cur.try_get("model_override").ok().flatten() };
    let row = sqlx::query("UPDATE chat_sessions SET title=$1, model_override=$2, updated_at=NOW() WHERE id=$3 RETURNING id, title, model_override, created_at, updated_at")
        .bind(&title).bind(&model_override).bind(sess).fetch_one(&state.pool).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_id=$1").bind(sess).fetch_one(&state.pool).await?;
    Ok(Json(ChatSessionResponse {
        id: sess.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or(title),
        source_id: sid.to_string(),
        model_override: row.try_get("model_override").ok().flatten(),
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(count),
    }))
}

async fn delete_session(
    State(state): State<AppState>,
    Path((source_id, session_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let sess = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    ensure_related(&state.pool, sid, sess).await?;
    sqlx::query("DELETE FROM chat_sessions WHERE id=$1").bind(sess).execute(&state.pool).await?;
    Ok(Json(json!({"success": true})))
}

async fn send_message(
    State(state): State<AppState>,
    Path((source_id, session_id)): Path<(String, String)>,
    Json(p): Json<SendMessageRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let sid = ensure_source(&state.pool, &source_id).await?;
    let sess = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    ensure_related(&state.pool, sid, sess).await?;

    // ponytail: persist + stream placeholder reply; model/provider execution pending.
    sqlx::query("INSERT INTO chat_messages (session_id, role, content) VALUES ($1,'human',$2)")
        .bind(sess).bind(&p.message).execute(&state.pool).await?;
    let answer = format!(
        "I can’t run source chat with a model yet (provider pipeline pending). You asked: {}",
        p.message.chars().take(400).collect::<String>()
    );
    sqlx::query("INSERT INTO chat_messages (session_id, role, content) VALUES ($1,'ai',$2)")
        .bind(sess).bind(&answer).execute(&state.pool).await?;
    sqlx::query("UPDATE chat_sessions SET updated_at=NOW(), model_override=COALESCE($2, model_override) WHERE id=$1")
        .bind(sess).bind(&p.model_override).execute(&state.pool).await?;

    let events = vec![
        Ok(Event::default().data(json!({"type":"user_message","content": p.message}).to_string())),
        Ok(Event::default().data(json!({"type":"ai_message","content": answer}).to_string())),
        Ok(Event::default().data(json!({"type":"context_indicators","data":{"sources":[sid.to_string()],"insights":[],"notes":[]}}).to_string())),
        Ok(Event::default().data(json!({"type":"complete"}).to_string())),
    ];
    Ok(Sse::new(stream::iter(events)))
}

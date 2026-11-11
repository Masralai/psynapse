use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
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
        .route("/chat/sessions", get(list_sessions).post(create_session))
        .route(
            "/chat/sessions/:session_id",
            get(get_session).put(update_session).delete(delete_session),
        )
        .route("/chat/execute", post(execute_chat))
        .route("/chat/context", post(build_context))
}

#[derive(Serialize)]
pub struct ChatSessionResponse {
    pub id: String,
    pub title: String,
    pub notebook_id: Option<String>,
    pub created: String,
    pub updated: String,
    pub message_count: Option<i64>,
    pub model_override: Option<String>,
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
pub struct ChatSessionWithMessagesResponse {
    pub id: String,
    pub title: String,
    pub notebook_id: Option<String>,
    pub created: String,
    pub updated: String,
    pub message_count: Option<i64>,
    pub model_override: Option<String>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub notebook_id: String,
    pub title: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub notebook_id: String,
}

#[derive(Deserialize)]
pub struct ExecuteChatRequest {
    pub session_id: String,
    pub message: String,
    #[serde(default)]
    pub context: Value,
    pub model_override: Option<String>,
}

#[derive(Serialize)]
pub struct ExecuteChatResponse {
    pub session_id: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
pub struct BuildContextRequest {
    pub notebook_id: String,
    pub context_config: ContextConfig,
}

#[derive(Deserialize, Default)]
pub struct ContextConfig {
    #[serde(default)]
    pub sources: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub notes: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
pub struct BuildContextResponse {
    pub context: Value,
    pub token_count: i64,
    pub char_count: i64,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }

async fn ensure_notebook(pool: &sqlx::PgPool, notebook_id: &str) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(notebook_id).map_err(|_| AppError::Http(axum::http::StatusCode::NOT_FOUND, "Notebook not found".into()))?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM notebooks WHERE id=$1").bind(id).fetch_optional(pool).await?;
    if exists.is_none() { return Err(AppError::Http(axum::http::StatusCode::NOT_FOUND, "Notebook not found".into())); }
    Ok(id)
}

async fn session_notebook_id(pool: &sqlx::PgPool, session_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, Uuid>("SELECT notebook_id FROM chat_session_notebooks WHERE session_id=$1 LIMIT 1")
        .bind(session_id).fetch_optional(pool).await.ok().flatten().map(|u| u.to_string())
}

async fn get_messages(pool: &sqlx::PgPool, session_id: Uuid) -> Result<Vec<ChatMessage>, AppError> {
    let rows = sqlx::query(
        "SELECT id, role, content, created_at FROM chat_messages WHERE session_id=$1 ORDER BY created_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| ChatMessage {
        id: r.get::<Uuid,_>("id").to_string(),
        msg_type: r.get::<String,_>("role"),
        content: r.get::<String,_>("content"),
        timestamp: Some(fmt(r.get("created_at"))),
    }).collect())
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<ChatSessionResponse>>, AppError> {
    let nid = ensure_notebook(&state.pool, &params.notebook_id).await?;
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.title, s.model_override, s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM chat_messages m WHERE m.session_id = s.id) as message_count
        FROM chat_sessions s
        JOIN chat_session_notebooks csn ON csn.session_id = s.id
        WHERE csn.notebook_id = $1
        ORDER BY s.updated_at DESC
        "#,
    )
    .bind(nid)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(|r| ChatSessionResponse {
        id: r.get::<Uuid,_>("id").to_string(),
        title: r.get::<Option<String>,_>("title").unwrap_or_else(|| "Untitled Session".into()),
        notebook_id: Some(nid.to_string()),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
        message_count: Some(r.get::<i64,_>("message_count")),
        model_override: r.try_get("model_override").ok().flatten(),
    }).collect()))
}

async fn create_session(
    State(state): State<AppState>,
    Json(p): Json<CreateSessionRequest>,
) -> Result<Json<ChatSessionResponse>, AppError> {
    let nid = ensure_notebook(&state.pool, &p.notebook_id).await?;
    let title = p.title.unwrap_or_else(|| "New Chat".into());
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO chat_sessions (title, model_override) VALUES ($1,$2) RETURNING id, title, model_override, created_at, updated_at",
    )
    .bind(&title).bind(&p.model_override).fetch_one(&mut *tx).await?;
    let sid: Uuid = row.get("id");
    sqlx::query("INSERT INTO chat_session_notebooks (session_id, notebook_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
        .bind(sid).bind(nid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(ChatSessionResponse {
        id: sid.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or(title),
        notebook_id: Some(nid.to_string()),
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(0),
        model_override: row.try_get("model_override").ok().flatten(),
    }))
}

async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ChatSessionWithMessagesResponse>, AppError> {
    let sid = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT id, title, model_override, created_at, updated_at FROM chat_sessions WHERE id=$1")
        .bind(sid).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let messages = get_messages(&state.pool, sid).await?;
    let notebook_id = session_notebook_id(&state.pool, sid).await;
    Ok(Json(ChatSessionWithMessagesResponse {
        id: sid.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or_else(|| "Untitled Session".into()),
        notebook_id,
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(messages.len() as i64),
        model_override: row.try_get("model_override").ok().flatten(),
        messages,
    }))
}

async fn update_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(p): Json<UpdateSessionRequest>,
) -> Result<Json<ChatSessionResponse>, AppError> {
    let sid = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    let cur = sqlx::query("SELECT title, model_override FROM chat_sessions WHERE id=$1")
        .bind(sid).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let title = p.title.or_else(|| cur.try_get("title").ok().flatten()).unwrap_or_else(|| "Untitled Session".into());
    let model_override = if p.model_override.is_some() { p.model_override } else { cur.try_get("model_override").ok().flatten() };
    let row = sqlx::query(
        "UPDATE chat_sessions SET title=$1, model_override=$2, updated_at=NOW() WHERE id=$3 RETURNING id, title, model_override, created_at, updated_at",
    )
    .bind(&title).bind(&model_override).bind(sid).fetch_one(&state.pool).await?;
    let notebook_id = session_notebook_id(&state.pool, sid).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_id=$1").bind(sid).fetch_one(&state.pool).await?;
    Ok(Json(ChatSessionResponse {
        id: sid.to_string(),
        title: row.get::<Option<String>,_>("title").unwrap_or(title),
        notebook_id,
        created: fmt(row.get("created_at")),
        updated: fmt(row.get("updated_at")),
        message_count: Some(count),
        model_override: row.try_get("model_override").ok().flatten(),
    }))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let sid = Uuid::parse_str(&session_id).map_err(|_| AppError::NotFound)?;
    let res = sqlx::query("DELETE FROM chat_sessions WHERE id=$1").bind(sid).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"success": true})))
}

async fn execute_chat(
    State(state): State<AppState>,
    Json(p): Json<ExecuteChatRequest>,
) -> Result<Json<ExecuteChatResponse>, AppError> {
    let sid = Uuid::parse_str(&p.session_id).map_err(|_| AppError::NotFound)?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM chat_sessions WHERE id=$1").bind(sid).fetch_optional(&state.pool).await?;
    if exists.is_none() { return Err(AppError::NotFound); }

    // ponytail: persist messages + deterministic placeholder reply; provider/graph execution pending.
    sqlx::query("INSERT INTO chat_messages (session_id, role, content) VALUES ($1,'human',$2)")
        .bind(sid).bind(&p.message).execute(&state.pool).await?;
    let answer = format!(
        "I can’t run model-backed chat yet (provider pipeline pending). I received: {}",
        p.message.chars().take(400).collect::<String>()
    );
    sqlx::query("INSERT INTO chat_messages (session_id, role, content) VALUES ($1,'ai',$2)")
        .bind(sid).bind(&answer).execute(&state.pool).await?;
    sqlx::query("UPDATE chat_sessions SET updated_at=NOW(), model_override=COALESCE($2, model_override) WHERE id=$1")
        .bind(sid).bind(&p.model_override).execute(&state.pool).await?;

    let messages = get_messages(&state.pool, sid).await?;
    Ok(Json(ExecuteChatResponse { session_id: p.session_id, messages }))
}

async fn build_context(
    State(state): State<AppState>,
    Json(p): Json<BuildContextRequest>,
) -> Result<Json<BuildContextResponse>, AppError> {
    let _nid = ensure_notebook(&state.pool, &p.notebook_id).await?;
    let mut sources_ctx: Vec<Value> = Vec::new();
    let mut notes_ctx: Vec<Value> = Vec::new();

    for (id, mode) in &p.context_config.sources {
        if mode == "not in" { continue; }
        if let Ok(uid) = Uuid::parse_str(id) {
            if let Ok(Some(row)) = sqlx::query("SELECT id, title, full_text FROM sources WHERE id=$1").bind(uid).fetch_optional(&state.pool).await {
                let full = mode == "full content";
                sources_ctx.push(json!({
                    "id": row.get::<Uuid,_>("id").to_string(),
                    "title": row.try_get::<Option<String>,_>("title").ok().flatten(),
                    "full_text": if full { row.try_get::<Option<String>,_>("full_text").ok().flatten() } else { None },
                    "mode": mode,
                }));
            }
        }
    }
    for (id, mode) in &p.context_config.notes {
        if mode == "not in" { continue; }
        if let Ok(uid) = Uuid::parse_str(id) {
            if let Ok(Some(row)) = sqlx::query("SELECT id, title, content FROM notes WHERE id=$1").bind(uid).fetch_optional(&state.pool).await {
                notes_ctx.push(json!({
                    "id": row.get::<Uuid,_>("id").to_string(),
                    "title": row.try_get::<Option<String>,_>("title").ok().flatten(),
                    "content": if mode == "full content" { row.try_get::<Option<String>,_>("content").ok().flatten() } else { None },
                    "mode": mode,
                }));
            }
        }
    }
    let context = json!({ "sources": sources_ctx, "notes": notes_ctx });
    let char_count = context.to_string().len() as i64;
    let token_count = (char_count / 4).max(0);
    Ok(Json(BuildContextResponse { context, token_count, char_count }))
}

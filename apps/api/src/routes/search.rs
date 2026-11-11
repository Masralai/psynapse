use axum::{extract::State, response::sse::{Event, Sse}, routing::post, Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/search", post(search))
        .route("/search/ask", post(ask))
        .route("/search/ask/simple", post(ask_simple))
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(rename = "type", default = "default_type")]
    pub search_type: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "t")]
    pub search_sources: bool,
    #[serde(default = "t")]
    pub search_notes: bool,
    #[serde(default)]
    pub minimum_score: f64,
    pub notebook_ids: Option<Vec<String>>,
}
fn default_type() -> String { "text".into() }
fn default_limit() -> i64 { 100 }
fn t() -> bool { true }

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<Value>,
    pub total_count: usize,
    pub search_type: String,
}

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
    pub strategy_model: String,
    pub answer_model: String,
    pub final_answer_model: String,
    pub notebook_ids: Option<Vec<String>>,
}

async fn search(State(state): State<AppState>, Json(req): Json<SearchRequest>) -> Result<Json<SearchResponse>, AppError> {
    if req.search_type == "vector" {
        return Err(AppError::BadRequest("Vector search requires embeddings/provider pipeline (not yet configured)".into()));
    }
    let q = req.query.trim().to_string();
    if q.is_empty() { return Ok(Json(SearchResponse { results: vec![], total_count: 0, search_type: "text".into() })); }
    let limit = req.limit.clamp(1, 1000);
    let notebook_ids: Vec<Uuid> = req.notebook_ids.unwrap_or_default().into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
    let mut results: Vec<Value> = Vec::new();

    if req.search_sources {
        let rows = sqlx::query(
            r#"
            SELECT s.id::text as id,
                   COALESCE(s.title,'Untitled source') as title,
                   s.id::text as parent_id,
                   ts_rank_cd(to_tsvector('english', coalesce(s.full_text,'') || ' ' || coalesce(s.title,'')), plainto_tsquery('english', $1))::float8 as final_score,
                   'source'::text as item_type,
                   s.created_at, s.updated_at
            FROM sources s
            WHERE to_tsvector('english', coalesce(s.full_text,'') || ' ' || coalesce(s.title,'')) @@ plainto_tsquery('english', $1)
              AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
            ORDER BY final_score DESC
            LIMIT $3
            "#,
        )
        .bind(&q).bind(&notebook_ids).bind(limit)
        .fetch_all(&state.pool).await?;
        for r in rows {
            results.push(json!({
                "id": r.get::<String,_>("id"),
                "title": r.get::<String,_>("title"),
                "parent_id": r.get::<String,_>("parent_id"),
                "final_score": r.get::<f64,_>("final_score"),
                "type": "source",
                "source_type": "source",
                "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
            }));
        }
    }
    if req.search_notes {
        let rows = sqlx::query(
            r#"
            SELECT n.id::text as id,
                   COALESCE(n.title,'Untitled note') as title,
                   n.id::text as parent_id,
                   ts_rank_cd(to_tsvector('english', coalesce(n.content,'') || ' ' || coalesce(n.title,'')), plainto_tsquery('english', $1))::float8 as final_score,
                   'note'::text as item_type,
                   n.created_at, n.updated_at
            FROM notes n
            WHERE to_tsvector('english', coalesce(n.content,'') || ' ' || coalesce(n.title,'')) @@ plainto_tsquery('english', $1)
              AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM note_notebooks nn WHERE nn.note_id=n.id AND nn.notebook_id = ANY($2)))
            ORDER BY final_score DESC
            LIMIT $3
            "#,
        )
        .bind(&q).bind(&notebook_ids).bind(limit)
        .fetch_all(&state.pool).await?;
        for r in rows {
            results.push(json!({
                "id": r.get::<String,_>("id"),
                "title": r.get::<String,_>("title"),
                "parent_id": r.get::<String,_>("parent_id"),
                "final_score": r.get::<f64,_>("final_score"),
                "type": "note",
                "source_type": "note",
                "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
            }));
        }
    }
    results.sort_by(|a,b| b.get("final_score").and_then(|v| v.as_f64()).partial_cmp(&a.get("final_score").and_then(|v| v.as_f64())).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);
    let total_count = results.len();
    Ok(Json(SearchResponse { results, total_count, search_type: "text".into() }))
}

async fn ask(State(_state): State<AppState>, Json(req): Json<AskRequest>) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    // ponytail: model/provider orchestration not wired yet; stream a clear error event instead of hanging UI.
    let msg = format!("Ask is not available yet in Rust API (models/providers pipeline pending). Question: {}", req.question.chars().take(120).collect::<String>());
    let events = vec![
        Ok(Event::default().data(json!({"type":"error","message": msg}).to_string())),
    ];
    Sse::new(stream::iter(events))
}

async fn ask_simple(State(_state): State<AppState>, Json(req): Json<AskRequest>) -> Result<Json<Value>, AppError> {
    Err(AppError::BadRequest(format!(
        "Ask is not available yet (model/provider pipeline pending) for: {}",
        req.question
    )))
}

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
    let q = req.query.trim().to_string();
    if q.is_empty() { return Ok(Json(SearchResponse { results: vec![], total_count: 0, search_type: req.search_type })); }
    let limit = req.limit.clamp(1, 1000);
    let notebook_ids: Vec<Uuid> = req.notebook_ids.unwrap_or_default().into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
    let mut results: Vec<Value> = Vec::new();

    if req.search_type == "vector" {
        // ponytail: pgvector similarity using default embedding model; best chunk per source + direct note embeddings.
        let qvecs = crate::providers::embed_texts(&state.pool, None, vec![q.clone()]).await?;
        let qlit = crate::providers::to_pgvector(&qvecs[0]);
        if req.search_sources {
            let rows = sqlx::query(
                r#"
                SELECT sub.id, sub.title, sub.parent_id, (1 - sub.dist)::float8 as final_score,
                       sub.created_at, sub.updated_at
                FROM (
                  SELECT s.id::text as id, COALESCE(s.title,'Untitled source') as title, s.id::text as parent_id,
                         MIN(se.embedding <=> $1::vector) as dist, s.created_at, s.updated_at
                  FROM source_embeddings se
                  JOIN sources s ON s.id = se.source_id
                  WHERE se.embedding IS NOT NULL
                    AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
                  GROUP BY s.id, s.title, s.created_at, s.updated_at
                  ORDER BY dist ASC
                  LIMIT $3
                ) sub
                "#,
            )
            .bind(&qlit).bind(&notebook_ids).bind(limit)
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
                SELECT n.id::text as id, COALESCE(n.title,'Untitled note') as title, n.id::text as parent_id,
                       (1 - (n.embedding <=> $1::vector))::float8 as final_score,
                       n.created_at, n.updated_at
                FROM notes n
                WHERE n.embedding IS NOT NULL
                  AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM note_notebooks nn WHERE nn.note_id=n.id AND nn.notebook_id = ANY($2)))
                ORDER BY n.embedding <=> $1::vector ASC
                LIMIT $3
                "#,
            )
            .bind(&qlit).bind(&notebook_ids).bind(limit)
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
    } else {
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
    }
    if req.minimum_score > 0.0 {
        results.retain(|r| r.get("final_score").and_then(|v| v.as_f64()).unwrap_or(0.0) >= req.minimum_score);
    }
    results.sort_by(|a,b| b.get("final_score").and_then(|v| v.as_f64()).partial_cmp(&a.get("final_score").and_then(|v| v.as_f64())).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);
    let total_count = results.len();
    Ok(Json(SearchResponse { results, total_count, search_type: req.search_type }))
}

async fn ask(State(state): State<AppState>, Json(req): Json<AskRequest>) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    // ponytail: minimal ask flow (FTS context + single final answer), not full multi-step strategy/agent parity.
    let q = req.question.trim().to_string();
    let strategy = json!({
        "type": "strategy",
        "reasoning": "Using direct text search over selected notebooks and answering with the configured model.",
        "searches": [{"term": q, "instructions": "Retrieve relevant source/note excerpts"}]
    });
    let ctx = gather_ask_context(&state.pool, &q, req.notebook_ids.clone()).await.unwrap_or_else(|_| "(context unavailable)".into());
    let system = "You answer questions using the provided knowledge base excerpts. Cite uncertainty if context is insufficient.";
    let user = format!("Question: {q}\n\nExcerpts:\n{ctx}");
    let model_id = Some(req.final_answer_model.as_str()).filter(|s| !s.trim().is_empty()).or(Some(req.answer_model.as_str())).or(Some(req.strategy_model.as_str()));
    let answer = match crate::providers::chat_completion(&state.pool, model_id, Some("default_chat_model"), system, &user).await {
        Ok(text) => text,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("No model configured") || msg.contains("No base URL") || msg.contains("Configured model not found") {
                format!("Ask needs a configured language model/provider. ({msg})")
            } else {
                let events = vec![Ok(Event::default().data(json!({"type":"error","message": msg}).to_string()))];
                return Sse::new(stream::iter(events));
            }
        }
    };
    let events = vec![
        Ok(Event::default().data(strategy.to_string())),
        Ok(Event::default().data(json!({"type":"final_answer","content": answer}).to_string())),
        Ok(Event::default().data(json!({"type":"complete","final_answer": answer}).to_string())),
    ];
    Sse::new(stream::iter(events))
}

async fn ask_simple(State(state): State<AppState>, Json(req): Json<AskRequest>) -> Result<Json<Value>, AppError> {
    let q = req.question.trim().to_string();
    let ctx = gather_ask_context(&state.pool, &q, req.notebook_ids.clone()).await.unwrap_or_else(|_| "(context unavailable)".into());
    let system = "You answer questions using the provided knowledge base excerpts. Cite uncertainty if context is insufficient.";
    let user = format!("Question: {q}\n\nExcerpts:\n{ctx}");
    let model_id = Some(req.final_answer_model.as_str()).filter(|s| !s.trim().is_empty());
    let answer = crate::providers::chat_completion(&state.pool, model_id, Some("default_chat_model"), system, &user).await?;
    Ok(json!({"answer": answer, "question": req.question}).into())
}

async fn gather_ask_context(pool: &sqlx::PgPool, q: &str, notebook_ids: Option<Vec<String>>) -> Result<String, AppError> {
    // ponytail: reuse simple FTS excerpts; no embeddings/ranking parity.
    let notebook_ids: Vec<Uuid> = notebook_ids.unwrap_or_default().into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
    let mut out = String::new();
    let srows = sqlx::query(
        r#"
        SELECT COALESCE(s.title,'Untitled source') as title, left(coalesce(s.full_text,''), 1200) as excerpt
        FROM sources s
        WHERE ($1 = '' OR to_tsvector('english', coalesce(s.full_text,'') || ' ' || coalesce(s.title,'')) @@ plainto_tsquery('english', $1))
          AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
        ORDER BY s.updated_at DESC
        LIMIT 5
        "#,
    )
    .bind(q).bind(&notebook_ids).fetch_all(pool).await?;
    for r in srows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Source] {title}\n{excerpt}\n")); }
    }
    let nrows = sqlx::query(
        r#"
        SELECT COALESCE(n.title,'Untitled note') as title, left(coalesce(n.content,''), 800) as excerpt
        FROM notes n
        WHERE ($1 = '' OR to_tsvector('english', coalesce(n.content,'') || ' ' || coalesce(n.title,'')) @@ plainto_tsquery('english', $1))
          AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM note_notebooks nn WHERE nn.note_id=n.id AND nn.notebook_id = ANY($2)))
        ORDER BY n.updated_at DESC
        LIMIT 5
        "#,
    )
    .bind(q).bind(&notebook_ids).fetch_all(pool).await?;
    for r in nrows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Note] {title}\n{excerpt}\n")); }
    }
    if out.trim().is_empty() { Ok("(no matching excerpts found)".into()) } else { Ok(out) }
}

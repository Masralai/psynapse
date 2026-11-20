use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::post,
    Json, Router,
};
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
    // ponytail: optional; defaults to search_sources so existing clients unchanged.
    pub search_insights: Option<bool>,
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

fn pick_ask_model_id(req: &AskRequest) -> Option<&str> {
    // ponytail: prefer final answer model, then answer, then strategy; caller supplies default slot fallback.
    Some(req.final_answer_model.as_str())
        .filter(|s| !s.trim().is_empty())
        .or(Some(req.answer_model.as_str()).filter(|s| !s.trim().is_empty()))
        .or(Some(req.strategy_model.as_str()).filter(|s| !s.trim().is_empty()))
}

async fn search(State(state): State<AppState>, Json(req): Json<SearchRequest>) -> Result<Json<SearchResponse>, AppError> {
    let q = req.query.trim().to_string();
    if q.is_empty() { return Ok(Json(SearchResponse { results: vec![], total_count: 0, search_type: req.search_type })); }
    let limit = req.limit.clamp(1, 1000);
    let search_insights = req.search_insights.unwrap_or(req.search_sources);
    let notebook_ids: Vec<Uuid> = req.notebook_ids.unwrap_or_default().into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
    let mut results: Vec<Value> = Vec::new();

    if req.search_type == "vector" {
        // ponytail: pgvector similarity using default embedding model; best chunk per source + direct note/insight embeddings.
        // ponytail: simple left() snippets for vector results (no lexical highlight/ranking parity).
        let qvecs = crate::providers::embed_texts(&state.pool, None, vec![q.clone()]).await?;
        let qlit = crate::providers::to_pgvector(&qvecs[0]);
        if req.search_sources {
            let rows = sqlx::query(
                r#"
                SELECT sub.id, sub.title, sub.parent_id, (1 - sub.dist)::float8 as final_score,
                       sub.snippet, sub.created_at, sub.updated_at
                FROM (
                  SELECT s.id::text as id, COALESCE(s.title,'Untitled source') as title, s.id::text as parent_id,
                         MIN(se.embedding <=> $1::vector) as dist,
                         left(coalesce(s.full_text,''), 220) as snippet,
                         s.created_at, s.updated_at
                  FROM source_embeddings se
                  JOIN sources s ON s.id = se.source_id
                  WHERE se.embedding IS NOT NULL
                    AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
                  GROUP BY s.id, s.title, s.created_at, s.updated_at, left(coalesce(s.full_text,''), 220)
                  ORDER BY dist ASC
                  LIMIT $3
                ) sub
                "#,
            )
            .bind(&qlit).bind(&notebook_ids).bind(limit)
            .fetch_all(&state.pool).await?;
            for r in rows {
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("source:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "source",
                    "source_type": "source",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
            }
        }
        if req.search_notes {
            let rows = sqlx::query(
                r#"
                SELECT n.id::text as id, COALESCE(n.title,'Untitled note') as title, n.id::text as parent_id,
                       (1 - (n.embedding <=> $1::vector))::float8 as final_score,
                       left(coalesce(n.content,''), 220) as snippet,
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
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("note:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "note",
                    "source_type": "note",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
            }
        }
        if search_insights {
            let rows = sqlx::query(
                r#"
                SELECT si.id::text as id,
                       COALESCE(NULLIF(si.insight_type,''), 'Insight') as title,
                       si.source_id::text as parent_id,
                       (1 - (si.embedding <=> $1::vector))::float8 as final_score,
                       left(coalesce(si.content,''), 220) as snippet,
                       si.created_at, si.updated_at
                FROM source_insights si
                JOIN sources s ON s.id = si.source_id
                WHERE si.embedding IS NOT NULL
                  AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
                ORDER BY si.embedding <=> $1::vector ASC
                LIMIT $3
                "#,
            )
            .bind(&qlit).bind(&notebook_ids).bind(limit)
            .fetch_all(&state.pool).await?;
            for r in rows {
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("source_insight:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "source_insight",
                    "source_type": "source_insight",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
            }
        }
    } else {
        // ponytail: ts_headline snippets for text search (basic highlighting/fragment config).
        if req.search_sources {
            let rows = sqlx::query(
                r#"
                SELECT s.id::text as id,
                       COALESCE(s.title,'Untitled source') as title,
                       s.id::text as parent_id,
                       ts_rank_cd(to_tsvector('english', coalesce(s.full_text,'') || ' ' || coalesce(s.title,'')), plainto_tsquery('english', $1))::float8 as final_score,
                       ts_headline('english', left(coalesce(s.full_text,'') || ' ' || coalesce(s.title,''), 4000), plainto_tsquery('english', $1),
                         'MaxFragments=1, MaxWords=25, MinWords=4, StartSel=<mark>, StopSel=</mark>') as snippet,
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
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("source:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "source",
                    "source_type": "source",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
            }
        }
        if req.search_notes {
            let rows = sqlx::query(
                r#"
                SELECT n.id::text as id,
                       COALESCE(n.title,'Untitled note') as title,
                       n.id::text as parent_id,
                       ts_rank_cd(to_tsvector('english', coalesce(n.content,'') || ' ' || coalesce(n.title,'')), plainto_tsquery('english', $1))::float8 as final_score,
                       ts_headline('english', left(coalesce(n.content,'') || ' ' || coalesce(n.title,''), 4000), plainto_tsquery('english', $1),
                         'MaxFragments=1, MaxWords=25, MinWords=4, StartSel=<mark>, StopSel=</mark>') as snippet,
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
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("note:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "note",
                    "source_type": "note",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
            }
        }
        if search_insights {
            let rows = sqlx::query(
                r#"
                SELECT si.id::text as id,
                       COALESCE(NULLIF(si.insight_type,''), 'Insight') as title,
                       si.source_id::text as parent_id,
                       ts_rank_cd(to_tsvector('english', coalesce(si.content,'') || ' ' || coalesce(si.insight_type,'')), plainto_tsquery('english', $1))::float8 as final_score,
                       ts_headline('english', left(coalesce(si.content,'') || ' ' || coalesce(si.insight_type,''), 4000), plainto_tsquery('english', $1),
                         'MaxFragments=1, MaxWords=25, MinWords=4, StartSel=<mark>, StopSel=</mark>') as snippet,
                       si.created_at, si.updated_at
                FROM source_insights si
                JOIN sources s ON s.id = si.source_id
                WHERE to_tsvector('english', coalesce(si.content,'') || ' ' || coalesce(si.insight_type,'')) @@ plainto_tsquery('english', $1)
                  AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
                ORDER BY final_score DESC
                LIMIT $3
                "#,
            )
            .bind(&q).bind(&notebook_ids).bind(limit)
            .fetch_all(&state.pool).await?;
            for r in rows {
                let id: String = r.get("id");
                let pid: String = r.get("parent_id");
                let snippet: String = r.try_get("snippet").unwrap_or_default();
                let mut obj = json!({
                    "id": format!("source_insight:{id}"),
                    "title": r.get::<String,_>("title"),
                    "parent_id": pid,
                    "final_score": r.get::<f64,_>("final_score"),
                    "type": "source_insight",
                    "source_type": "source_insight",
                    "created": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339(),
                    "updated": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at").to_rfc3339(),
                });
                if !snippet.trim().is_empty() { obj["matches"] = json!([snippet]); }
                results.push(obj);
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
    // ponytail: minimal ask flow (vector retrieval when possible, FTS fallback) + single final answer.
    let q = req.question.trim().to_string();
    let strategy = json!({
        "type": "strategy",
        "reasoning": "Retrieving relevant excerpts (vector when available, otherwise text search) and answering with the configured model.",
        "searches": [{"term": q, "instructions": "Retrieve relevant source/note/insight excerpts"}]
    });
    let ctx = gather_ask_context(&state.pool, &q, req.notebook_ids.clone()).await.unwrap_or_else(|_| "(context unavailable)".into());
    let system = "You answer questions using the provided knowledge base excerpts. Cite uncertainty if context is insufficient.";
    let user = format!("Question: {q}\n\nExcerpts:\n{ctx}");
    let model_id = pick_ask_model_id(&req);
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
        Ok(Event::default().data(json!({"type":"answer","content": answer}).to_string())),
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
    let model_id = pick_ask_model_id(&req);
    let answer = crate::providers::chat_completion(&state.pool, model_id, Some("default_chat_model"), system, &user).await?;
    Ok(json!({"answer": answer, "question": req.question}).into())
}

async fn gather_ask_context(pool: &sqlx::PgPool, q: &str, notebook_ids: Option<Vec<String>>) -> Result<String, AppError> {
    // ponytail: try vector retrieval when embeddings available, else FTS excerpts (sources/notes/insights).
    if !q.trim().is_empty() {
        if let Ok(ctx) = gather_ask_context_vector(pool, q, notebook_ids.clone()).await {
            if !ctx.trim().is_empty() && ctx != "(no matching excerpts found)" {
                return Ok(ctx);
            }
        }
    }
    gather_ask_context_fts(pool, q, notebook_ids).await
}

async fn gather_ask_context_vector(pool: &sqlx::PgPool, q: &str, notebook_ids: Option<Vec<String>>) -> Result<String, AppError> {
    let notebook_ids: Vec<Uuid> = notebook_ids.unwrap_or_default().into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();
    let qvecs = crate::providers::embed_texts(pool, None, vec![q.to_string()]).await?;
    let Some(qv) = qvecs.first() else { return Ok(String::new()); };
    let qlit = crate::providers::to_pgvector(qv);
    let mut out = String::new();

    // ponytail: best chunk per source for diversity (not top-k chunks globally).
    let srows = sqlx::query(
        r#"
        SELECT title, excerpt FROM (
          SELECT DISTINCT ON (s.id)
            COALESCE(s.title,'Untitled source') as title,
            left(coalesce(se.content,''), 1200) as excerpt,
            (se.embedding <=> $1::vector) as dist
          FROM source_embeddings se
          JOIN sources s ON s.id = se.source_id
          WHERE se.embedding IS NOT NULL
            AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
          ORDER BY s.id, dist ASC
        ) sub
        ORDER BY dist ASC
        LIMIT 5
        "#,
    )
    .bind(&qlit).bind(&notebook_ids).fetch_all(pool).await?;
    for r in srows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Source] {title}\n{excerpt}\n")); }
    }

    let nrows = sqlx::query(
        r#"
        SELECT COALESCE(n.title,'Untitled note') as title, left(coalesce(n.content,''), 800) as excerpt
        FROM notes n
        WHERE n.embedding IS NOT NULL
          AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM note_notebooks nn WHERE nn.note_id=n.id AND nn.notebook_id = ANY($2)))
        ORDER BY n.embedding <=> $1::vector ASC
        LIMIT 5
        "#,
    )
    .bind(&qlit).bind(&notebook_ids).fetch_all(pool).await?;
    for r in nrows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Note] {title}\n{excerpt}\n")); }
    }

    let irows = sqlx::query(
        r#"
        SELECT COALESCE(NULLIF(si.insight_type,''), 'Insight') as title, left(coalesce(si.content,''), 800) as excerpt
        FROM source_insights si
        JOIN sources s ON s.id = si.source_id
        WHERE si.embedding IS NOT NULL
          AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
        ORDER BY si.embedding <=> $1::vector ASC
        LIMIT 5
        "#,
    )
    .bind(&qlit).bind(&notebook_ids).fetch_all(pool).await?;
    for r in irows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Insight] {title}\n{excerpt}\n")); }
    }

    Ok(if out.trim().is_empty() { "(no matching excerpts found)".into() } else { out })
}

async fn gather_ask_context_fts(pool: &sqlx::PgPool, q: &str, notebook_ids: Option<Vec<String>>) -> Result<String, AppError> {
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
    let irows = sqlx::query(
        r#"
        SELECT COALESCE(NULLIF(si.insight_type,''), 'Insight') as title, left(coalesce(si.content,''), 800) as excerpt
        FROM source_insights si
        JOIN sources s ON s.id = si.source_id
        WHERE ($1 = '' OR to_tsvector('english', coalesce(si.content,'') || ' ' || coalesce(si.insight_type,'')) @@ plainto_tsquery('english', $1))
          AND (cardinality($2::uuid[]) = 0 OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id=s.id AND sn.notebook_id = ANY($2)))
        ORDER BY si.updated_at DESC
        LIMIT 5
        "#,
    )
    .bind(q).bind(&notebook_ids).fetch_all(pool).await?;
    for r in irows {
        let title: String = r.get("title");
        let excerpt: String = r.try_get("excerpt").unwrap_or_default();
        if !excerpt.trim().is_empty() { out.push_str(&format!("\n[Insight] {title}\n{excerpt}\n")); }
    }
    if out.trim().is_empty() { Ok("(no matching excerpts found)".into()) } else { Ok(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask_req(final_m: &str, answer_m: &str, strategy_m: &str) -> AskRequest {
        AskRequest {
            question: "q".into(),
            strategy_model: strategy_m.into(),
            answer_model: answer_m.into(),
            final_answer_model: final_m.into(),
            notebook_ids: None,
        }
    }

    #[test]
    fn pick_ask_model_id_prefers_final_then_answer_then_strategy() {
        let r = ask_req("final", "answer", "strategy");
        assert_eq!(pick_ask_model_id(&r), Some("final"));
        let r = ask_req("  ", "answer", "strategy");
        assert_eq!(pick_ask_model_id(&r), Some("answer"));
        let r = ask_req("", " ", "strategy");
        assert_eq!(pick_ask_model_id(&r), Some("strategy"));
        let r = ask_req("", "", "");
        assert_eq!(pick_ask_model_id(&r), None);
    }
}

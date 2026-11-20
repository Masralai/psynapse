use crate::errors::AppError;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

pub async fn enqueue(
    pool: &PgPool,
    app_name: &str,
    command_name: &str,
    input: Value,
) -> Result<Uuid, AppError> {
    let row = sqlx::query(
        r#"
        INSERT INTO jobs (app_name, command_name, input, status)
        VALUES ($1, $2, $3, 'queued')
        RETURNING id
        "#,
    )
    .bind(app_name)
    .bind(command_name)
    .bind(input)
    .fetch_one(pool)
    .await?;
    Ok(row.get("id"))
}

pub fn parse_job_id(s: &str) -> Result<Uuid, AppError> {
    let s = s.strip_prefix("command:").unwrap_or(s);
    Uuid::parse_str(s).map_err(|_| AppError::NotFound)
}

async fn job_canceled(pool: &PgPool, id: Uuid) -> bool {
    // ponytail: cooperative cancellation check for long-running handlers.
    matches!(
        sqlx::query_scalar::<_, String>("SELECT status FROM jobs WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .as_deref(),
        Some("canceled") | Some("cancelled")
    )
}

pub async fn status_payload(pool: &PgPool, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, app_name, command_name, status, result, error_message, retry_count, max_retries, created_at, started_at, completed_at
        FROM jobs WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let status: String = row.get("status");
    let result: Option<Value> = row.get("result");
    let error_message: Option<String> = row.get("error_message");
    let processing_info = json!({
        "started_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at").map(|t| t.to_rfc3339()),
        "completed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at").map(|t| t.to_rfc3339()),
        "error": error_message,
        "result": result,
    });
    let id = row.get::<Uuid, _>("id").to_string();
    let app_name: String = row.get("app_name");
    let command_name: String = row.get("command_name");

    Ok(json!({
        "id": id,
        "job_id": id,
        "command_id": id,
        "app_name": app_name,
        "app": app_name,
        "command_name": command_name,
        "command": command_name,
        "name": command_name,
        "status": status,
        "result": result,
        "error_message": error_message,
        "retry_count": row.get::<i32, _>("retry_count"),
        "max_retries": row.get::<i32, _>("max_retries"),
        "created": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        "updated": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at")
            .unwrap_or_else(|| row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"))
            .to_rfc3339(),
        "progress": processing_info,
    }))
}

pub fn as_command_id(job_id: Uuid) -> String {
    format!("command:{}", job_id)
}

// ponytail: single polling loop, SKIP LOCKED claim; handlers with partial provider/embedding execution.
// In-flight cancellation is cooperative/best-effort: completion/failure updates are conditional so canceled jobs are not overwritten.
pub async fn run_worker(pool: PgPool, poll_interval_secs: u64, retry_backoff_secs: u64) {
    let poll = Duration::from_secs(poll_interval_secs.max(1));
    let backoff = Duration::from_secs(retry_backoff_secs.max(1));
    loop {
        match process_next(&pool).await {
            Ok(true) => {}
            Ok(false) => tokio::time::sleep(poll).await,
            Err(e) => {
                tracing::error!("job worker error: {e}");
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

async fn process_next(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let job = sqlx::query(
        r#"
        WITH next AS (
            SELECT id FROM jobs
            WHERE status = 'queued'
            ORDER BY created_at
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE jobs j
        SET status = 'running', started_at = NOW()
        FROM next
        WHERE j.id = next.id
        RETURNING j.id, j.command_name, j.input
        "#,
    )
    .fetch_optional(pool)
    .await?;
    let Some(job) = job else { return Ok(false) };

    let id: Uuid = job.get("id");
    let cmd: String = job.get("command_name");
    let input: Value = job.get("input");
    touch_source_running(pool, &cmd, &input).await;

    match dispatch(pool, id, &cmd, &input).await {
        Ok(result) => {
            let updated = sqlx::query(
                "UPDATE jobs SET status='completed', result=$2, error_message=NULL, completed_at=NOW() WHERE id=$1 AND status='running'",
            )
            .bind(id)
            .bind(result)
            .execute(pool)
            .await?;
            if updated.rows_affected() == 0 {
                tracing::info!("job {id} not marked completed (likely canceled/interrupted)");
            }
        }
        Err(err) => {
            let row = sqlx::query(
                r#"
                UPDATE jobs
                SET retry_count = retry_count + 1,
                    error_message = $2,
                    status = CASE WHEN retry_count + 1 < max_retries THEN 'queued' ELSE 'failed' END,
                    completed_at = CASE WHEN retry_count + 1 < max_retries THEN NULL ELSE NOW() END,
                    started_at = CASE WHEN retry_count + 1 < max_retries THEN NULL ELSE started_at END
                WHERE id = $1 AND status = 'running'
                RETURNING status
                "#,
            )
            .bind(id)
            .bind(err.clone())
            .fetch_optional(pool)
            .await?;
            if let Some(row) = row {
                let status: String = row.get("status");
                touch_source_terminal(pool, &cmd, &input, &status, &err).await;
            } else {
                tracing::info!("job {id} not marked failed/requeued (likely canceled/interrupted)");
            }
        }
    }
    Ok(true)
}

async fn touch_source_running(pool: &PgPool, cmd: &str, input: &Value) {
    if cmd != "process_source" {
        return;
    }
    if let Some(sid) = source_id(input) {
        let _ = sqlx::query(
            "UPDATE sources SET status='running', processing_info = jsonb_build_object('async', true, 'running', true), updated_at=NOW() WHERE id=$1",
        )
        .bind(sid)
        .execute(pool)
        .await;
    }
}

async fn touch_source_terminal(pool: &PgPool, cmd: &str, input: &Value, status: &str, err: &str) {
    if cmd != "process_source" {
        return;
    }
    if let Some(sid) = source_id(input) {
        let info = json!({"async": true, "status": status, "error": err});
        let _ = sqlx::query("UPDATE sources SET status=$1, processing_info=$2, updated_at=NOW() WHERE id=$3")
            .bind(status)
            .bind(info)
            .bind(sid)
            .execute(pool)
            .await;
    }
}

fn source_id(input: &Value) -> Option<Uuid> {
    input
        .get("source_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

async fn dispatch(pool: &PgPool, job_id: Uuid, command: &str, input: &Value) -> Result<Value, String> {
    if job_canceled(pool, job_id).await {
        return Err("canceled".into());
    }
    match command {
        "process_source" => process_source(pool, job_id, input).await,
        "embed_source" => embed_source_job(pool, input).await,
        "embed_note" => embed_note_job(pool, input).await,
        "rebuild_embeddings" => rebuild_embeddings_job(pool, job_id, input).await,
        "run_transformation" => run_transformation_job(pool, input).await,
        "generate_podcast" => generate_podcast_job(pool, job_id, input).await,
        _ => Ok(json!({"skipped": true, "command": command})),
    }
}

fn is_embedding_unconfigured(err: &str) -> bool {
    err.contains("No embedding model configured")
        || err.contains("No base URL/API configuration for provider")
        || err.contains("Configured model not found")
}

async fn embed_source_job(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    match embed_source_by_id(pool, sid).await {
        Ok(chunks_created) => Ok(json!({"status":"completed","source_id": sid.to_string(), "chunks_created": chunks_created})),
        Err(e) if is_embedding_unconfigured(&e) => Ok(json!({"status":"skipped","source_id": sid.to_string(), "chunks_created": 0, "note": e})),
        Err(e) => Err(e),
    }
}

async fn embed_note_job(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let nid = input
        .get("note_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| "missing note_id".to_string())?;
    match embed_note_by_id(pool, nid).await {
        Ok(_) => Ok(json!({"status":"completed","note_id": nid.to_string(), "chunks_created": 1})),
        Err(e) if is_embedding_unconfigured(&e) => Ok(json!({"status":"skipped","note_id": nid.to_string(), "chunks_created": 0, "note": e})),
        Err(e) => Err(e),
    }
}

async fn rebuild_embeddings_job(pool: &PgPool, job_id: Uuid, input: &Value) -> Result<Value, String> {
    // ponytail: sequential inline rebuild using default embedding model; no per-item subjobs/progress streaming.
    let include_sources = input.get("include_sources").and_then(|v| v.as_bool()).unwrap_or(true);
    let include_notes = input.get("include_notes").and_then(|v| v.as_bool()).unwrap_or(true);
    let include_insights = input.get("include_insights").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut sources = 0i64;
    let mut notes = 0i64;
    let mut insights = 0i64;
    let mut failed = 0i64;
    let mut processed = 0i64;
    let mut skipped_unconfigured = false;

    if include_sources {
        let rows = sqlx::query("SELECT id FROM sources WHERE full_text IS NOT NULL AND btrim(full_text) <> ''")
            .fetch_all(pool).await.map_err(|e| e.to_string())?;
        for r in rows {
            if job_canceled(pool, job_id).await { return Err("canceled".into()); }
            let id: Uuid = r.get("id");
            match embed_source_by_id(pool, id).await {
                Ok(_) => { sources += 1; processed += 1; }
                Err(e) if is_embedding_unconfigured(&e) => { skipped_unconfigured = true; tracing::warn!("rebuild skipped (embedding unconfigured): {e}"); break; }
                Err(e) => { tracing::warn!("rebuild embed source {id} failed: {e}"); failed += 1; processed += 1; }
            }
        }
    }
    if !skipped_unconfigured && include_notes {
        let rows = sqlx::query("SELECT id FROM notes WHERE content IS NOT NULL AND btrim(content) <> ''")
            .fetch_all(pool).await.map_err(|e| e.to_string())?;
        for r in rows {
            if job_canceled(pool, job_id).await { return Err("canceled".into()); }
            let id: Uuid = r.get("id");
            match embed_note_by_id(pool, id).await {
                Ok(_) => { notes += 1; processed += 1; }
                Err(e) if is_embedding_unconfigured(&e) => { skipped_unconfigured = true; tracing::warn!("rebuild skipped (embedding unconfigured): {e}"); break; }
                Err(e) => { tracing::warn!("rebuild embed note {id} failed: {e}"); failed += 1; processed += 1; }
            }
        }
    }
    if !skipped_unconfigured && include_insights {
        let rows = sqlx::query("SELECT id FROM source_insights WHERE content IS NOT NULL AND btrim(content) <> ''")
            .fetch_all(pool).await.map_err(|e| e.to_string())?;
        for r in rows {
            if job_canceled(pool, job_id).await { return Err("canceled".into()); }
            let id: Uuid = r.get("id");
            match embed_insight_by_id(pool, id).await {
                Ok(_) => { insights += 1; processed += 1; }
                Err(e) if is_embedding_unconfigured(&e) => { skipped_unconfigured = true; tracing::warn!("rebuild skipped (embedding unconfigured): {e}"); break; }
                Err(e) => { tracing::warn!("rebuild embed insight {id} failed: {e}"); failed += 1; processed += 1; }
            }
        }
    }

    Ok(json!({
        "status": "completed",
        "total_items": processed,
        "processed": processed,
        "jobs_submitted": processed,
        "failed": failed,
        "stats": {"sources": sources, "notes": notes, "insights": insights, "failed": failed},
        "note": if skipped_unconfigured { Some("skipped remaining items: embedding model/provider not configured") } else { None }
    }))
}

async fn embed_source_by_id(pool: &PgPool, sid: Uuid) -> Result<i64, String> {
    let row = sqlx::query("SELECT full_text FROM sources WHERE id=$1")
        .bind(sid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "source not found".to_string())?;
    let text: String = row.try_get::<Option<String>, _>("full_text").ok().flatten().unwrap_or_default();
    if text.trim().is_empty() {
        return Err("Source has no text content".into());
    }
    let chunks = crate::providers::chunk_text(&text, 1200, 200);
    if chunks.is_empty() {
        return Err("No embeddable content".into());
    }
    let embeddings = crate::providers::embed_texts(pool, None, chunks.clone())
        .await
        .map_err(|e| e.to_string())?;
    if embeddings.len() != chunks.len() {
        return Err("Embedding count mismatch".into());
    }
    sqlx::query("DELETE FROM source_embeddings WHERE source_id=$1")
        .bind(sid)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut created = 0i64;
    for (i, (content, emb)) in chunks.into_iter().zip(embeddings.into_iter()).enumerate() {
        let vec = crate::providers::to_pgvector(&emb);
        sqlx::query(
            "INSERT INTO source_embeddings (source_id, content, embedding, chunk_order) VALUES ($1,$2,$3::vector,$4)",
        )
        .bind(sid)
        .bind(content)
        .bind(vec)
        .bind(i as i32)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        created += 1;
    }
    Ok(created)
}

async fn embed_note_by_id(pool: &PgPool, nid: Uuid) -> Result<(), String> {
    let row = sqlx::query("SELECT content FROM notes WHERE id=$1")
        .bind(nid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "note not found".to_string())?;
    let text: String = row.try_get::<Option<String>, _>("content").ok().flatten().unwrap_or_default();
    if text.trim().is_empty() {
        return Err("Note has no content".into());
    }
    let snippet: String = text.chars().take(8000).collect();
    let embs = crate::providers::embed_texts(pool, None, vec![snippet])
        .await
        .map_err(|e| e.to_string())?;
    let emb = embs.into_iter().next().ok_or_else(|| "No embedding returned".to_string())?;
    let vec = crate::providers::to_pgvector(&emb);
    sqlx::query("UPDATE notes SET embedding=$1::vector, updated_at=NOW() WHERE id=$2")
        .bind(vec)
        .bind(nid)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn embed_insight_by_id(pool: &PgPool, iid: Uuid) -> Result<(), String> {
    let row = sqlx::query("SELECT content FROM source_insights WHERE id=$1")
        .bind(iid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "insight not found".to_string())?;
    let text: String = row.try_get::<Option<String>, _>("content").ok().flatten().unwrap_or_default();
    if text.trim().is_empty() {
        return Err("Insight has no content".into());
    }
    let snippet: String = text.chars().take(8000).collect();
    let embs = crate::providers::embed_texts(pool, None, vec![snippet])
        .await
        .map_err(|e| e.to_string())?;
    let emb = embs.into_iter().next().ok_or_else(|| "No embedding returned".to_string())?;
    let vec = crate::providers::to_pgvector(&emb);
    sqlx::query("UPDATE source_insights SET embedding=$1::vector, updated_at=NOW() WHERE id=$2")
        .bind(vec)
        .bind(iid)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn run_transformation_inline(pool: &PgPool, sid: Uuid, tid: Uuid, source_text: &str) -> Result<Uuid, String> {
    let t = sqlx::query("SELECT name, title, prompt, model_id FROM transformations WHERE id=$1")
        .bind(tid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "transformation not found".to_string())?;
    if source_text.trim().is_empty() {
        return Err("Source has no text content".into());
    }
    let tname: String = t.try_get("name").unwrap_or_else(|_| "transformation".into());
    let ttitle: String = t.try_get("title").unwrap_or_else(|_| tname.clone());
    let prompt: String = t.try_get("prompt").unwrap_or_default();
    let model_id = t
        .try_get::<Option<Uuid>, _>("model_id")
        .ok()
        .flatten()
        .map(|u| u.to_string());

    let system = "You apply the given transformation prompt to the source text. Return only the transformed result.";
    let user = format!(
        "Transformation: {ttitle}\n\nInstructions:\n{prompt}\n\nSource:\n{}",
        source_text.chars().take(12000).collect::<String>()
    );
    let content = match crate::providers::chat_completion(pool, model_id.as_deref(), Some("default_transformation_model"), system, &user).await {
        Ok(text) => text,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("No model configured") || msg.contains("No base URL") {
                format!(
                    "[transformation pending provider pipeline]\n\nTransformation: {}\nPrompt:\n{}\n\nSource excerpt:\n{}",
                    ttitle,
                    prompt,
                    source_text.chars().take(1200).collect::<String>()
                )
            } else {
                return Err(msg);
            }
        }
    };
    let row = sqlx::query(
        "INSERT INTO source_insights (source_id, insight_type, content) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(sid)
    .bind(&tname)
    .bind(&content)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let iid: Uuid = row.get("id");
    // ponytail: best-effort insight embedding when embedding model is configured.
    if let Err(e) = embed_insight_by_id(pool, iid).await {
        if !is_embedding_unconfigured(&e) {
            tracing::warn!("insight embed failed for {iid}: {e}");
        }
    }
    Ok(iid)
}

async fn run_transformation_job(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    let tid = input
        .get("transformation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| "missing transformation_id".to_string())?;
    let src = sqlx::query("SELECT full_text FROM sources WHERE id=$1")
        .bind(sid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "source not found".to_string())?;
    let full_text: String = src.try_get::<Option<String>, _>("full_text").ok().flatten().unwrap_or_default();
    if full_text.trim().is_empty() {
        return Err("Source has no text content".into());
    }
    let iid = run_transformation_inline(pool, sid, tid, &full_text).await?;
    Ok(json!({"status":"completed","insight_id": iid.to_string(), "source_id": sid.to_string()}))
}

async fn process_source(pool: &PgPool, job_id: Uuid, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    let cs = input.get("content_state").cloned().unwrap_or_else(|| json!({}));
    let (mut full_text, note, mut inferred_title) = extract_content(&cs).await?;

    if full_text.as_deref().map(str::trim).unwrap_or("").is_empty() {
        full_text = None;
    }

    let char_count = full_text.as_ref().map(|t| t.chars().count()).unwrap_or(0);
    let info = json!({
        "async": true,
        "completed": true,
        "note": note,
        "char_count": char_count,
        "has_full_text": full_text.is_some(),
        "completed_at": chrono::Utc::now().to_rfc3339()
    });
    let res = sqlx::query(
        r#"
        UPDATE sources
        SET full_text = COALESCE($1, full_text),
            title = CASE
                WHEN ($2::text IS NOT NULL AND (title IS NULL OR btrim(title) = '' OR title = 'Processing...')) THEN $2
                ELSE title
            END,
            status = 'completed',
            processing_info = $3,
            updated_at = NOW()
        WHERE id = $4
        "#,
    )
    .bind(full_text.clone())
    .bind(inferred_title.take())
    .bind(info)
    .bind(sid)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    if res.rows_affected() == 0 {
        return Err("source not found".to_string());
    }

    // ponytail: best-effort embedding/transformation integration during async processing when configured.
    let mut embedded_chunks: Option<i64> = None;
    let mut insights_created: Vec<String> = Vec::new();
    if let Some(text) = &full_text {
        if job_canceled(pool, job_id).await { return Err("canceled".into()); }
        if input.get("embed").and_then(|v| v.as_bool()).unwrap_or(false) {
            match embed_source_by_id(pool, sid).await {
                Ok(n) => embedded_chunks = Some(n),
                Err(e) => tracing::warn!("process_source embed skipped/failed for {sid}: {e}"),
            }
        }
        if let Some(tids) = input.get("transformations").and_then(|v| v.as_array()) {
            for tval in tids {
                if job_canceled(pool, job_id).await { return Err("canceled".into()); }
                if let Some(tid_str) = tval.as_str() {
                    if let Ok(tid) = Uuid::parse_str(tid_str) {
                        match run_transformation_inline(pool, sid, tid, text).await {
                            Ok(iid) => insights_created.push(iid.to_string()),
                            Err(e) => tracing::warn!("process_source transformation {tid_str} failed for {sid}: {e}"),
                        }
                    }
                }
            }
        }
    }

    Ok(json!({
        "source_id": sid.to_string(),
        "status": "completed",
        "note": note,
        "char_count": char_count,
        "has_full_text": full_text.is_some(),
        "embedded_chunks": embedded_chunks,
        "insights_created": insights_created
    }))
}

fn max_extract_bytes() -> usize {
    // ponytail: simple cap for in-worker extraction; richer streaming/chunked extraction pending.
    std::env::var("SOURCE_EXTRACT_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10 * 1024 * 1024)
}

fn looks_like_html(s: &str) -> bool {
    let t = s.trim_start().to_ascii_lowercase();
    t.starts_with("<!doctype html") || t.starts_with("<html") || (t.contains("<body") && t.contains("</"))
}

async fn extract_content(cs: &Value) -> Result<(Option<String>, String, Option<String>), String> {
    if let Some(content) = cs
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| cs.get("text").and_then(|v| v.as_str()))
    {
        let title = content
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().chars().take(80).collect());
        let (text, note) = normalize_text_content(content, "direct", None);
        return Ok((text, note, title));
    }

    if let Some(url) = cs.get("url").and_then(|v| v.as_str()) {
        return fetch_url_text(url).await;
    }

    let file_path = cs
        .get("file_path")
        .and_then(|v| v.as_str())
        .or_else(|| cs.pointer("/asset/file_path").and_then(|v| v.as_str()));
    if let Some(path) = file_path {
        return extract_file_text(path).await;
    }

    Ok((None, "no extractable content in this pass".into(), None))
}

fn normalize_text_content(raw: &str, source_kind: &str, ext_hint: Option<&str>) -> (Option<String>, String) {
    normalize_text_content_with_max(raw, source_kind, ext_hint, max_extract_bytes())
}

fn normalize_text_content_with_max(raw: &str, source_kind: &str, ext_hint: Option<&str>, max: usize) -> (Option<String>, String) {
    let mut text = raw.to_string();
    let mut notes: Vec<String> = vec![format!("{source_kind} content extracted")];
    let ext = ext_hint.unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "html" | "htm") || looks_like_html(&text) {
        let stripped = html_to_text(&text);
        if !stripped.trim().is_empty() {
            text = stripped;
            notes.push("html stripped to text".into());
        }
    }
    if text.len() > max {
        let mut cut = max.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        notes.push(format!("truncated to {cut} bytes"));
    }
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return (None, format!("{source_kind} extracted but empty"));
    }
    (Some(trimmed), notes.join("; "))
}

async fn fetch_url_text(url: &str) -> Result<(Option<String>, String, Option<String>), String> {
    // ponytail: basic fetch + content-type-aware text/HTML extraction with size cap; richer extraction/rendered content deferred.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("psynapse-api/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("url fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("url fetch failed: status {}", resp.status()));
    }
    let max = max_extract_bytes();
    if let Some(len) = resp.content_length() {
        if len as usize > max.saturating_mul(4) {
            return Ok((
                None,
                format!("url content too large to extract ({} bytes)", len),
                Some(url.to_string()),
            ));
        }
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("url read failed: {e}"))?;
    let mut truncated = false;
    let mut data = bytes.to_vec();
    if data.len() > max {
        data.truncate(max);
        truncated = true;
    }
    let body = String::from_utf8_lossy(&data).to_string();
    let title = extract_html_title(&body).or_else(|| Some(url.to_string()));

    let is_textual = content_type.is_empty()
        || content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("html");
    if !is_textual {
        return Ok((
            None,
            format!("url content-type not text-extractable ({content_type})"),
            title,
        ));
    }

    let mut text = body;
    let mut notes = vec!["url content fetched".to_string()];
    if content_type.contains("html") || looks_like_html(&text) {
        let stripped = html_to_text(&text);
        if !stripped.trim().is_empty() {
            text = stripped;
            notes.push("html stripped to text".into());
        }
    }
    if truncated {
        notes.push("truncated during fetch".into());
    }
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Ok((None, "url fetched but no text extracted".into(), title));
    }
    Ok((Some(trimmed), notes.join("; "), title))
}

fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    let raw = &html[start..end];
    let t = html_to_text(raw);
    let t = t.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.chars().take(120).collect())
    }
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(200_000));
    let mut in_tag = false;
    let mut skip = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' => {
                in_tag = false;
                let t = tag.to_ascii_lowercase();
                if t.starts_with("script") || t.starts_with("style") || t.starts_with("noscript") {
                    skip = true;
                }
                if t.starts_with("/script") || t.starts_with("/style") || t.starts_with("/noscript") {
                    skip = false;
                }
                if matches!(
                    t.as_str(),
                    "br" | "br/" | "p" | "/p" | "div" | "/div" | "li" | "/li" | "tr" | "/tr"
                        | "h1" | "/h1" | "h2" | "/h2" | "h3" | "/h3" | "h4" | "/h4"
                        | "h5" | "/h5" | "h6" | "/h6" | "section" | "/section"
                        | "article" | "/article" | "ul" | "/ul" | "ol" | "/ol"
                ) {
                    out.push('\n');
                }
            }
            _ if in_tag => {
                if tag.len() < 24 {
                    tag.push(ch);
                }
            }
            _ if !skip => out.push(ch),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'");
    let mut result = String::with_capacity(decoded.len());
    let mut last_was_space = false;
    let mut nl_run = 0usize;
    for ch in decoded.chars() {
        if ch == '\n' || ch == '\r' {
            nl_run += 1;
            last_was_space = false;
            continue;
        }
        if nl_run > 0 {
            let n = if nl_run >= 2 { 2 } else { 1 };
            for _ in 0..n { result.push('\n'); }
            nl_run = 0;
            last_was_space = false;
        }
        if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(ch);
            last_was_space = false;
        }
    }
    if nl_run > 0 {
        let n = if nl_run >= 2 { 2 } else { 1 };
        for _ in 0..n { result.push('\n'); }
    }
    // ponytail: preserve block breaks, collapse inline whitespace.
    result
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

async fn extract_file_text(path: &str) -> Result<(Option<String>, String, Option<String>), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err("uploaded file not found".into());
    }
    let meta = tokio::fs::metadata(p)
        .await
        .map_err(|e| format!("file stat failed: {e}"))?;
    let title = p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let max = max_extract_bytes();
    if meta.len() as usize > max.saturating_mul(4) {
        return Ok((
            None,
            format!("file too large to extract ({} bytes)", meta.len()),
            title,
        ));
    }
    let bytes = tokio::fs::read(p)
        .await
        .map_err(|e| format!("file read failed: {e}"))?;
    Ok(interpret_extracted_file_bytes(&ext, title, bytes, max))
}

fn is_likely_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "txt" | "md" | "markdown" | "csv" | "tsv" | "json" | "jsonl" | "xml" | "html" | "htm"
            | "yaml" | "yml" | "log" | "rst" | "tex" | "bib" | "srt" | "vtt" | "svg"
            | "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
            | "cs" | "rb" | "php" | "swift" | "kt" | "scala" | "toml" | "css" | "scss" | "sass"
            | "sql" | "sh" | "bash" | "zsh" | "env" | "ini" | "conf" | "cfg" | "properties"
    )
}

fn binary_extraction_note(ext: &str) -> String {
    // ponytail: clearer note for formats needing specialized parsers; no parser deps added.
    match ext {
        "pdf" => "pdf extraction pending pipeline".into(),
        "doc" | "docx" | "odt" | "rtf" => "document extraction pending pipeline".into(),
        "ppt" | "pptx" | "key" => "presentation extraction pending pipeline".into(),
        "xls" | "xlsx" | "ods" => "spreadsheet extraction pending pipeline".into(),
        "mp3" | "wav" | "m4a" | "mp4" | "mov" | "webm" | "ogg" | "flac" | "aac" => {
            "media/transcript extraction pending pipeline".into()
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "heic" => {
            "image/ocr extraction pending pipeline".into()
        }
        "" => "binary extraction pending pipeline".into(),
        _ => format!("binary/{ext} extraction pending pipeline"),
    }
}

fn interpret_extracted_file_bytes(
    ext: &str,
    title: Option<String>,
    mut data: Vec<u8>,
    max: usize,
) -> (Option<String>, String, Option<String>) {
    let likely_text = is_likely_text_ext(ext);
    let mut truncated = false;
    if data.len() > max {
        data.truncate(max);
        truncated = true;
    }

    match String::from_utf8(data) {
        Ok(text) => {
            if !likely_text && text.trim().is_empty() {
                return (None, "file extracted but empty".into(), title);
            }
            let (normalized, mut note) = normalize_text_content_with_max(&text, "file", Some(ext), max);
            if truncated {
                note = format!("{note}; truncated during read");
            }
            if normalized.is_none() {
                return (
                    None,
                    if truncated {
                        "file extracted but empty after truncation".into()
                    } else {
                        "file extracted but empty".into()
                    },
                    title,
                );
            }
            (
                normalized,
                if likely_text {
                    note
                } else {
                    format!("{note}; decoded as text")
                },
                title,
            )
        }
        Err(_) => (None, binary_extraction_note(ext), title),
    }
}

async fn generate_podcast_job(pool: &PgPool, job_id: Uuid, input: &Value) -> Result<Value, String> {
    let eid = input
        .get("episode_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| "missing episode_id".to_string())?;
    let row = sqlx::query("SELECT name, briefing, content, episode_profile, speaker_profile FROM episodes WHERE id=$1")
        .bind(eid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "episode not found".to_string())?;
    let name: String = row.try_get("name").unwrap_or_else(|_| "Episode".into());
    let mut content: String = row.try_get("content").unwrap_or_default();
    let briefing: String = row.try_get("briefing").unwrap_or_default();
    let ep_snap: Value = row.try_get("episode_profile").unwrap_or_else(|_| json!({}));

    // ponytail: minimal content fallback from notebook sources when no direct content provided.
    if content.trim().is_empty() {
        if let Some(nb_str) = input.get("notebook_id").and_then(|v| v.as_str()) {
            if let Ok(nid) = Uuid::parse_str(nb_str) {
                let rows = sqlx::query(
                    r#"SELECT coalesce(s.title,'Untitled source') as title, left(coalesce(s.full_text,''), 2000) as excerpt
                       FROM sources s JOIN source_notebooks sn ON sn.source_id=s.id
                       WHERE sn.notebook_id=$1 ORDER BY s.updated_at DESC LIMIT 5"#,
                )
                .bind(nid)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                let mut buf = String::new();
                for r in rows {
                    let t: String = r.try_get("title").unwrap_or_default();
                    let ex: String = r.try_get("excerpt").unwrap_or_default();
                    if !ex.trim().is_empty() { buf.push_str(&format!("\n## {t}\n{ex}\n")); }
                }
                content = buf;
            }
        }
    }
    if content.trim().is_empty() {
        return Err("No content available for podcast generation".into());
    }

    let outline_model = ep_snap.get("outline_llm").and_then(|v| v.as_str()).map(str::to_string);
    let transcript_model = ep_snap.get("transcript_llm").and_then(|v| v.as_str()).map(str::to_string)
        .or_else(|| outline_model.clone());
    let num_segments = ep_snap.get("num_segments").and_then(|v| v.as_i64()).unwrap_or(5);

    let outline_system = "You create podcast outlines. Return valid JSON with key 'segments' (array of objects with 'title' and 'description').";
    let outline_user = format!(
        "Episode: {name}\nSegments: {num_segments}\nBriefing:\n{briefing}\n\nContent:\n{}",
        content.chars().take(12000).collect::<String>()
    );
    let outline_text = crate::providers::chat_completion(pool, outline_model.as_deref(), Some("default_chat_model"), outline_system, &outline_user)
        .await
        .map_err(|e| e.to_string())?;
    if job_canceled(pool, job_id).await { return Err("canceled".into()); }
    let outline_json: Value = serde_json::from_str(outline_text.trim())
        .or_else(|_| extract_json_object(&outline_text))
        .unwrap_or_else(|_| json!({"segments":[{"title": name, "description": outline_text}]}));

    let transcript_system = "You write podcast transcripts. Return valid JSON with key 'transcript' (array of objects with 'speaker' and 'text').";
    let transcript_user = format!(
        "Episode: {name}\nBriefing:\n{briefing}\n\nOutline:\n{}\n\nContent:\n{}",
        serde_json::to_string(&outline_json).unwrap_or_default(),
        content.chars().take(10000).collect::<String>()
    );
    let transcript_text = crate::providers::chat_completion(pool, transcript_model.as_deref(), Some("default_chat_model"), transcript_system, &transcript_user)
        .await
        .map_err(|e| e.to_string())?;
    if job_canceled(pool, job_id).await { return Err("canceled".into()); }
    let transcript_json: Value = serde_json::from_str(transcript_text.trim())
        .or_else(|_| extract_json_object(&transcript_text))
        .unwrap_or_else(|_| json!({"transcript":[{"speaker":"Host","text": transcript_text}]}));

    // ponytail: TTS/audio synthesis pipeline not implemented; persist outline/transcript only.
    let res = sqlx::query(
        "UPDATE episodes SET outline=$1, transcript=$2, updated_at=NOW() WHERE id=$3",
    )
    .bind(&outline_json)
    .bind(&transcript_json)
    .bind(eid)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    if res.rows_affected() == 0 {
        return Err("episode not found".into());
    }

    Ok(json!({
        "status":"completed",
        "episode_id": eid.to_string(),
        "outline_generated": true,
        "transcript_generated": true,
        "audio_generated": false,
        "note": "Audio synthesis/TTS pending; outline and transcript generated."
    }))
}

fn extract_json_object(s: &str) -> Result<Value, serde_json::Error> {
    let trimmed = s.trim();
    // ponytail: strip common markdown code fences before parsing.
    let candidate = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        lines.next(); // fence line (``` or ```json)
        let mut body = String::new();
        for line in lines {
            if line.trim_start().starts_with("```") {
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        body
    } else {
        trimmed.to_string()
    };
    let t = candidate.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Ok(v);
    }
    if let Some(start) = t.find('{') {
        if let Some(end) = t.rfind('}') {
            if end > start {
                return serde_json::from_str(&t[start..=end]);
            }
        }
    }
    serde_json::from_str(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_html_detects_common_docs() {
        assert!(looks_like_html("<!doctype html><html></html>"));
        assert!(looks_like_html("  <HTML><body>x</body></HTML>"));
        assert!(looks_like_html("<html><body>hi</body></html>"));
        assert!(!looks_like_html("plain text"));
    }

    #[test]
    fn html_to_text_strips_tags_scripts_and_decodes_entities() {
        let html = "<html><head><title>Hi</title><script>alert(1)</script></head><body><h1>Hello</h1><p>A &amp; B<br>Line2</p></body></html>";
        let t = html_to_text(html);
        assert!(t.contains("Hello"));
        assert!(t.contains("A & B"));
        assert!(t.contains("Line2"));
        assert!(!t.contains("alert"));
        assert!(!t.contains("<h1>"));
    }

    #[test]
    fn extract_html_title_reads_title_tag() {
        assert_eq!(
            extract_html_title("<html><head><title>  My Page  </title></head></html>").as_deref(),
            Some("My Page")
        );
        assert!(extract_html_title("<html><body>no title</body></html>").is_none());
    }

    #[test]
    fn normalize_text_content_strips_html_by_ext_and_trims() {
        let (text, note) = normalize_text_content_with_max("<p>Hello</p><p>World</p>", "file", Some("html"), 10_000);
        let text = text.unwrap();
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(note.contains("html stripped"));
    }

    #[test]
    fn normalize_text_content_truncates_with_marker() {
        let raw = "abcdefghij";
        let (text, note) = normalize_text_content_with_max(raw, "direct", None, 5);
        assert!(text.is_some());
        assert_eq!(text.unwrap(), "abcde");
        assert!(note.contains("truncated to 5 bytes"));
    }

    #[test]
    fn extract_json_object_parses_plain_fenced_and_embedded() {
        let v = extract_json_object(r#"{"a":1}"#).unwrap();
        assert_eq!(v["a"], 1);
        let fenced = "```json\n{\"b\":2}\n```";
        let v2 = extract_json_object(fenced).unwrap();
        assert_eq!(v2["b"], 2);
        let embedded = "here you go:\n{\"c\":3}\nthanks";
        let v3 = extract_json_object(embedded).unwrap();
        assert_eq!(v3["c"], 3);
    }

    #[test]
    fn interpret_extracted_file_bytes_handles_text_html_and_binary_notes() {
        let (text, note, title) = interpret_extracted_file_bytes(
            "txt",
            Some("doc".into()),
            b"hello world".to_vec(),
            10_000,
        );
        assert_eq!(text.as_deref(), Some("hello world"));
        assert_eq!(title.as_deref(), Some("doc"));
        assert!(note.contains("file content extracted"));

        let (html_text, note, _) = interpret_extracted_file_bytes(
            "html",
            None,
            b"<html><body><p>Hello</p></body></html>".to_vec(),
            10_000,
        );
        assert!(html_text.unwrap_or_default().contains("Hello"));
        assert!(note.contains("html stripped"));

        let (none, note, _) = interpret_extracted_file_bytes("pdf", Some("paper".into()), vec![0, 1, 2, 3, 255], 10_000);
        assert!(none.is_none());
        assert_eq!(note, "pdf extraction pending pipeline");

        let (none, note, _) = interpret_extracted_file_bytes("docx", None, vec![0, 159, 146, 150], 10_000);
        assert!(none.is_none());
        assert!(note.contains("document extraction pending pipeline"));
    }

    #[test]
    fn interpret_extracted_file_bytes_decodes_unknown_utf8_and_truncates() {
        let (text, note, _) = interpret_extracted_file_bytes("weirdext", None, b"plain text".to_vec(), 10_000);
        assert_eq!(text.as_deref(), Some("plain text"));
        assert!(note.contains("decoded as text"));

        let (text, note, _) = interpret_extracted_file_bytes("txt", None, b"abcdefghij".to_vec(), 5);
        assert_eq!(text.as_deref(), Some("abcde"));
        assert!(note.contains("truncated"));
    }

    #[test]
    fn is_likely_text_ext_covers_common_subtitle_and_source_formats() {
        assert!(is_likely_text_ext("srt"));
        assert!(is_likely_text_ext("vtt"));
        assert!(is_likely_text_ext("svg"));
        assert!(is_likely_text_ext("tsv"));
        assert!(!is_likely_text_ext("pdf"));
        assert!(!is_likely_text_ext("mp4"));
    }

    #[test]
    fn parse_job_id_accepts_plain_and_command_prefixed() {
        let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(parse_job_id(&id.to_string()).unwrap(), id);
        assert_eq!(
            parse_job_id("command:11111111-1111-1111-1111-111111111111").unwrap(),
            id
        );
        assert!(parse_job_id("not-a-uuid").is_err());
        assert!(parse_job_id("command:not-a-uuid").is_err());
    }

    #[test]
    fn as_command_id_formats_prefixed_identifier() {
        let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(
            as_command_id(id),
            "command:11111111-1111-1111-1111-111111111111"
        );
    }
}

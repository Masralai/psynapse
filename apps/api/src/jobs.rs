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

pub async fn status_payload(pool: &PgPool, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, status, result, error_message, created_at, started_at, completed_at
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

    Ok(json!({
        "job_id": row.get::<Uuid, _>("id").to_string(),
        "status": status,
        "result": result,
        "error_message": error_message,
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
pub async fn run_worker(pool: PgPool) {
    loop {
        match process_next(&pool).await {
            Ok(true) => {}
            Ok(false) => tokio::time::sleep(Duration::from_secs(2)).await,
            Err(e) => {
                tracing::error!("job worker error: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
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

    match dispatch(pool, &cmd, &input).await {
        Ok(result) => {
            sqlx::query(
                "UPDATE jobs SET status='completed', result=$2, error_message=NULL, completed_at=NOW() WHERE id=$1",
            )
            .bind(id)
            .bind(result)
            .execute(pool)
            .await?;
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
                WHERE id = $1
                RETURNING status
                "#,
            )
            .bind(id)
            .bind(err.clone())
            .fetch_one(pool)
            .await?;
            let status: String = row.get("status");
            touch_source_terminal(pool, &cmd, &input, &status, &err).await;
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

async fn dispatch(pool: &PgPool, command: &str, input: &Value) -> Result<Value, String> {
    match command {
        "process_source" => process_source(pool, input).await,
        "embed_source" => embed_source_job(pool, input).await,
        "embed_note" => embed_note_job(pool, input).await,
        "rebuild_embeddings" => rebuild_embeddings_job(pool, input).await,
        "run_transformation" => run_transformation_job(pool, input).await,
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

async fn rebuild_embeddings_job(pool: &PgPool, input: &Value) -> Result<Value, String> {
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

async fn process_source(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    let cs = input.get("content_state").cloned().unwrap_or_else(|| json!({}));
    let (mut full_text, note, mut inferred_title) = extract_content(&cs).await?;

    if full_text.as_deref().map(str::trim).unwrap_or("").is_empty() {
        full_text = None;
    }

    let info = json!({
        "async": true,
        "completed": true,
        "note": note,
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
        if input.get("embed").and_then(|v| v.as_bool()).unwrap_or(false) {
            match embed_source_by_id(pool, sid).await {
                Ok(n) => embedded_chunks = Some(n),
                Err(e) => tracing::warn!("process_source embed skipped/failed for {sid}: {e}"),
            }
        }
        if let Some(tids) = input.get("transformations").and_then(|v| v.as_array()) {
            for tval in tids {
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
        "has_full_text": full_text.is_some(),
        "embedded_chunks": embedded_chunks,
        "insights_created": insights_created
    }))
}

async fn extract_content(cs: &Value) -> Result<(Option<String>, String, Option<String>), String> {
    if let Some(content) = cs
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| cs.get("text").and_then(|v| v.as_str()))
    {
        let title = content.lines().find(|l| !l.trim().is_empty()).map(|l| l.trim().chars().take(80).collect());
        return Ok((Some(content.to_string()), "text content stored".into(), title));
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

async fn fetch_url_text(url: &str) -> Result<(Option<String>, String, Option<String>), String> {
    // ponytail: basic fetch + naive HTML-to-text; richer extraction/rendered content deferred.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| format!("url fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("url fetch failed: status {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| format!("url read failed: {e}"))?;
    let title = extract_html_title(&body).or_else(|| Some(url.to_string()));
    let text = html_to_text(&body);
    if text.trim().is_empty() {
        return Ok((None, "url fetched but no text extracted".into(), title));
    }
    Ok((Some(text), "url content fetched".into(), title))
}

fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    let t = html[start..end].trim();
    if t.is_empty() { None } else { Some(t.chars().take(120).collect()) }
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(200_000));
    let mut in_tag = false;
    let mut skip = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match ch {
            '<' => { in_tag = true; tag.clear(); }
            '>' => {
                in_tag = false;
                let t = tag.to_ascii_lowercase();
                if t.starts_with("script") || t.starts_with("style") { skip = true; }
                if t.starts_with("/script") || t.starts_with("/style") { skip = false; }
                if matches!(t.as_str(), "br" | "br/" | "p" | "/p" | "div" | "/div" | "li" | "/li" | "tr" | "/tr" | "h1" | "/h1" | "h2" | "/h2" | "h3" | "/h3") {
                    out.push('\n');
                }
            }
            _ if in_tag => { if tag.len() < 20 { tag.push(ch); } }
            _ if !skip => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn extract_file_text(path: &str) -> Result<(Option<String>, String, Option<String>), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err("uploaded file not found".into());
    }
    let title = p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let likely_text = matches!(ext.as_str(), "txt"|"md"|"markdown"|"csv"|"json"|"xml"|"html"|"htm"|"yaml"|"yml"|"log"|"rs"|"py"|"js"|"ts"|"tsx"|"jsx"|"go"|"java"|"c"|"cpp"|"h"|"toml");
    let bytes = tokio::fs::read(p).await.map_err(|e| format!("file read failed: {e}"))?;
    match String::from_utf8(bytes) {
        Ok(text) if likely_text || !text.trim().is_empty() => Ok((Some(text), "file text extracted".into(), title)),
        Ok(text) if text.trim().is_empty() => Ok((None, "file extracted but empty".into(), title)),
        Ok(text) => Ok((Some(text), "file decoded as text".into(), title)),
        Err(_) => Ok((None, format!("binary/{ext} extraction pending pipeline"), title)),
    }
}

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

// ponytail: single polling loop, SKIP LOCKED claim, minimal process_source handler; extraction/transforms/embeddings deferred.
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
        "embed_source" => embed_source_job(input).await,
        "embed_note" => embed_note_job(input).await,
        "rebuild_embeddings" => rebuild_embeddings_job(input).await,
        "run_transformation" => run_transformation_job(pool, input).await,
        _ => Ok(json!({"skipped": true, "command": command})),
    }
}

async fn embed_source_job(input: &Value) -> Result<Value, String> {
    // ponytail: embedding provider/vector generation not wired; job completes as placeholder.
    Ok(json!({"status":"completed","source_id": input.get("source_id"), "note":"embedding provider pending"}))
}

async fn embed_note_job(input: &Value) -> Result<Value, String> {
    Ok(json!({"status":"completed","note_id": input.get("note_id"), "note":"embedding provider pending"}))
}

async fn rebuild_embeddings_job(input: &Value) -> Result<Value, String> {
    let total = input.get("total_items").and_then(|v| v.as_i64()).unwrap_or(0);
    Ok(json!({
        "status":"completed",
        "total_items": total,
        "processed": 0,
        "jobs_submitted": 0,
        "stats": {"sources": 0, "notes": 0, "insights": 0, "failed": 0},
        "note":"embedding provider pending"
    }))
}

async fn run_transformation_job(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    let tid = input
        .get("transformation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| "missing transformation_id".to_string())?;
    let t = sqlx::query("SELECT name, title, prompt FROM transformations WHERE id=$1")
        .bind(tid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "transformation not found".to_string())?;
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
    let tname: String = t.try_get("name").unwrap_or_else(|_| "transformation".into());
    let ttitle: String = t.try_get("title").unwrap_or_else(|_| tname.clone());
    let prompt: String = t.try_get("prompt").unwrap_or_default();
    // ponytail: no LLM execution; persist placeholder insight so async insight flow is observable.
    let content = format!(
        "[transformation pending provider pipeline]\n\nTransformation: {}\nPrompt:\n{}\n\nSource excerpt:\n{}",
        ttitle,
        prompt,
        full_text.chars().take(1200).collect::<String>()
    );
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
    Ok(json!({"status":"completed","insight_id": iid.to_string(), "source_id": sid.to_string()}))
}

async fn process_source(pool: &PgPool, input: &Value) -> Result<Value, String> {
    let sid = source_id(input).ok_or_else(|| "missing source_id".to_string())?;
    let cs = input.get("content_state").cloned().unwrap_or_else(|| json!({}));
    let (mut full_text, note, mut inferred_title) = extract_content(&cs).await?;

    // ponytail: no transforms/embeddings yet; keep job successful when we can store or explain content.
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
    Ok(json!({"source_id": sid.to_string(), "status": "completed", "note": note, "has_full_text": full_text.is_some()}))
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
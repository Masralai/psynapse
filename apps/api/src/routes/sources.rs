use crate::{errors::AppError, jobs, state::AppState};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, Method, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::{
    net::IpAddr,
    path::{Path as FsPath, PathBuf},
};
use tokio::io::AsyncWriteExt;
use url::Url;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources).post(create_source))
        .route("/sources/json", post(create_source_json))
        .route(
            "/sources/:source_id",
            get(get_source).put(update_source).delete(delete_source),
        )
        .route("/sources/:source_id/download", get(download_source_file).head(download_source_file))
        .route("/sources/:source_id/status", get(get_source_status))
        .route("/sources/:source_id/retry", post(retry_source))
        .route("/sources/:source_id/insights", get(list_source_insights))
}

#[derive(Serialize)]
pub struct AssetModel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize)]
pub struct SourceListResponse {
    pub id: String,
    pub title: Option<String>,
    pub topics: Option<Vec<String>>,
    pub asset: Option<AssetModel>,
    pub embedded: bool,
    pub embedded_chunks: i64,
    pub insights_count: i64,
    pub created: String,
    pub updated: String,
    pub file_available: Option<bool>,
    pub command_id: Option<String>,
    pub status: Option<String>,
    pub processing_info: Option<Value>,
}

#[derive(Serialize)]
pub struct SourceDetailResponse {
    #[serde(flatten)]
    pub base: SourceListResponse,
    pub full_text: Option<String>,
    pub notebooks: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct SourceStatusResponse {
    pub status: Option<String>,
    pub message: String,
    pub processing_info: Option<Value>,
    pub command_id: Option<String>,
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

#[derive(Deserialize)]
pub struct ListParams {
    pub notebook_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

#[derive(Deserialize)]
pub struct SourceCreate {
    pub notebook_id: Option<String>,
    pub notebooks: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub title: Option<String>,
    pub transformations: Option<Vec<String>>,
    #[serde(default)]
    pub embed: bool,
    #[serde(default)]
    pub delete_source: bool,
    #[serde(default)]
    pub async_processing: bool,
}

#[derive(Deserialize)]
pub struct SourceUpdate {
    pub title: Option<String>,
    pub topics: Option<Vec<String>>,
}

fn fmt_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339()
}
fn fmt_opt(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(fmt_ts)
}

fn parse_asset(v: Option<Value>) -> Option<AssetModel> {
    let obj = v?.as_object()?.clone();
    let file_path = obj.get("file_path").and_then(|x| x.as_str()).map(str::to_string);
    let url = obj.get("url").and_then(|x| x.as_str()).map(str::to_string);
    if file_path.is_none() && url.is_none() { None } else { Some(AssetModel { file_path, url }) }
}
fn parse_topics(v: Option<Value>) -> Option<Vec<String>> {
    Some(v?.as_array()?.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
}
fn parse_processing_info(raw: Option<Value>, cmd_row: Option<&sqlx::postgres::PgRow>) -> Option<Value> {
    // ponytail: prefer stored processing_info; fall back to job timestamps/error for UI polling.
    if raw.is_some() { return raw; }
    cmd_row.map(|r| json!({
        "started_at": r.try_get::<Option<DateTime<Utc>>, _>("started_at").ok().flatten().map(fmt_ts),
        "completed_at": r.try_get::<Option<DateTime<Utc>>, _>("completed_at").ok().flatten().map(fmt_ts),
        "error": r.try_get::<Option<String>, _>("error_message").ok().flatten(),
        "result": r.try_get::<Option<Value>, _>("result").ok().flatten(),
    }))
}

fn order_clause(sort_by: &str, sort_order: &str) -> &'static str {
    let asc = sort_order.eq_ignore_ascii_case("asc");
    match sort_by {
        "title" => if asc { "s.title ASC NULLS LAST" } else { "s.title DESC NULLS LAST" },
        "created" => if asc { "s.created_at ASC" } else { "s.created_at DESC" },
        "insights_count" => if asc { "insights_count ASC" } else { "insights_count DESC" },
        "embedded" => if asc { "embedded_chunks ASC" } else { "embedded_chunks DESC" },
        _ => if asc { "s.updated_at ASC" } else { "s.updated_at DESC" },
    }
}

fn upload_dir() -> PathBuf {
    PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./data/uploads".into()))
}
fn in_uploads(path: &FsPath) -> bool {
    let root = upload_dir().canonicalize().unwrap_or_else(|_| upload_dir());
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p.starts_with(root)
}
fn sanitize_filename(name: &str) -> String {
    let base = FsPath::new(name).file_name().and_then(|s| s.to_str()).unwrap_or("upload.bin");
    let safe: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_() ".contains(c) { c } else { '_' }).collect();
    if safe.trim().is_empty() { "upload.bin".into() } else { safe }
}
async fn unique_upload_path(name: &str) -> Result<PathBuf, AppError> {
    let dir = upload_dir();
    tokio::fs::create_dir_all(&dir).await.map_err(|e| AppError::Http(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let safe = sanitize_filename(name);
    let p = FsPath::new(&safe);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("upload").to_string();
    let ext = p.extension().and_then(|s| s.to_str()).map(|s| format!(".{s}")).unwrap_or_default();
    let mut candidate = dir.join(&safe);
    let mut i = 0u32;
    loop {
        match tokio::fs::OpenOptions::new().write(true).create_new(true).open(&candidate).await {
            Ok(_) => { tokio::fs::remove_file(&candidate).await.ok(); return Ok(candidate); }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                i += 1;
                candidate = dir.join(format!("{stem} ({i}){ext}"));
            }
            Err(e) => return Err(AppError::Http(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        }
    }
}
fn is_private_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".local") { return true; }
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || v4.octets()[0] == 169,
        Ok(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unique_local() || v6.is_unspecified(),
        Err(_) => false,
    }
}
fn validate_url(url: &str) -> Result<(), AppError> {
    let parsed = Url::parse(url).map_err(|_| AppError::BadRequest("Invalid URL".into()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(AppError::BadRequest("Only http/https URLs are allowed".into())),
    }
    let host = parsed.host_str().ok_or_else(|| AppError::BadRequest("Invalid URL host".into()))?;
    if is_private_host(host) { return Err(AppError::BadRequest("URL host is not allowed".into())); }
    Ok(())
}

async fn row_to_list_item(pool: &sqlx::PgPool, r: &sqlx::postgres::PgRow) -> Result<SourceListResponse, AppError> {
    let embedded_chunks: i64 = r.try_get("embedded_chunks").unwrap_or(0);
    let command_id: Option<Uuid> = r.try_get("command_id").ok().flatten();
    let mut status: Option<String> = r.try_get("status").ok().flatten();
    let mut processing_info: Option<Value> = r.try_get("processing_info").ok().flatten();
    if let Some(cid) = command_id {
        if let Ok(Some(j)) = sqlx::query("SELECT status, result, error_message, started_at, completed_at FROM jobs WHERE id=$1").bind(cid).fetch_optional(pool).await {
            status = status.or_else(|| j.try_get("status").ok());
            processing_info = processing_info.or_else(|| parse_processing_info(None, Some(&j)));
        }
    }
    let asset = parse_asset(r.try_get("asset").ok().flatten());
    Ok(SourceListResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        title: r.try_get("title").ok().flatten(),
        topics: parse_topics(r.try_get("topics").ok().flatten()),
        embedded: embedded_chunks > 0,
        embedded_chunks,
        insights_count: r.try_get("insights_count").unwrap_or(0),
        created: fmt_ts(r.get("created_at")),
        updated: fmt_ts(r.get("updated_at")),
        file_available: asset.as_ref().and_then(|x| x.file_path.as_ref()).map(|p| {
            let path = FsPath::new(p);
            in_uploads(path) && path.exists()
        }),
        asset,
        command_id: command_id.map(|u| u.to_string()),
        status,
        processing_info,
    })
}

async fn list_sources(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<SourceListResponse>>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let order = order_clause(params.sort_by.as_deref().unwrap_or("updated"), params.sort_order.as_deref().unwrap_or("desc"));
    let notebook_id = params.notebook_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let sql = format!(
        r#"
        SELECT s.id, s.title, s.topics, s.asset, s.command_id, s.status, s.processing_info,
               s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM source_embeddings se WHERE se.source_id = s.id) AS embedded_chunks,
               (SELECT COUNT(*) FROM source_insights si WHERE si.source_id = s.id) AS insights_count
        FROM sources s
        WHERE ($1::uuid IS NULL OR EXISTS (SELECT 1 FROM source_notebooks sn WHERE sn.source_id = s.id AND sn.notebook_id = $1))
        ORDER BY {order} LIMIT $2 OFFSET $3
        "#
    );
    let rows = sqlx::query(&sql).bind(notebook_id).bind(limit).bind(offset).fetch_all(&state.pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows { out.push(row_to_list_item(&state.pool, r).await?); }
    Ok(Json(out))
}

async fn get_source_detail(pool: &sqlx::PgPool, id: Uuid) -> Result<SourceDetailResponse, AppError> {
    let row = sqlx::query(
        r#"
        SELECT s.id, s.title, s.topics, s.asset, s.command_id, s.status, s.processing_info,
               s.full_text, s.created_at, s.updated_at,
               (SELECT COUNT(*) FROM source_embeddings se WHERE se.source_id = s.id) AS embedded_chunks,
               (SELECT COUNT(*) FROM source_insights si WHERE si.source_id = s.id) AS insights_count
        FROM sources s WHERE s.id = $1
        "#,
    ).bind(id).fetch_optional(pool).await?.ok_or(AppError::NotFound)?;
    let base = row_to_list_item(pool, &row).await?;
    let nb_rows = sqlx::query("SELECT notebook_id FROM source_notebooks WHERE source_id=$1").bind(id).fetch_all(pool).await?;
    Ok(SourceDetailResponse {
        base,
        full_text: row.try_get("full_text").ok().flatten(),
        notebooks: Some(nb_rows.into_iter().map(|r| r.get::<Uuid,_>("notebook_id").to_string()).collect()),
    })
}

async fn get_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<SourceDetailResponse>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let _ = sqlx::query("UPDATE sources SET last_viewed_at = NOW() WHERE id = $1").bind(id).execute(&state.pool).await;
    Ok(Json(get_source_detail(&state.pool, id).await?))
}

async fn get_source_status(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<SourceStatusResponse>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT command_id, status, processing_info FROM sources WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let command_id: Option<Uuid> = row.try_get("command_id").ok();
    if command_id.is_none() {
        return Ok(Json(SourceStatusResponse {
            status: row.try_get("status").ok(),
            message: "No processing job associated with this source".into(),
            processing_info: row.try_get("processing_info").ok(),
            command_id: None,
        }));
    }
    let cid = command_id.unwrap();
    let job = jobs::status_payload(&state.pool, cid).await.ok();
    let status = job.as_ref().and_then(|j| j.get("status")).and_then(|s| s.as_str()).map(str::to_string)
        .or_else(|| row.try_get("status").ok());
    let processing_info = job.as_ref().and_then(|j| j.get("progress")).cloned().or_else(|| row.try_get("processing_info").ok());
    let message = match status.as_deref() {
        Some("completed") => "Source processing completed successfully",
        Some("failed") => "Source processing failed",
        Some("running") => "Source processing in progress",
        Some("queued") | Some("new") => "Source processing queued",
        _ => "Source processing status unknown",
    }.to_string();
    Ok(Json(SourceStatusResponse { status, message, processing_info, command_id: Some(cid.to_string()) }))
}

async fn download_source_file(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Response, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT asset, title FROM sources WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let asset = parse_asset(row.try_get("asset").ok()).ok_or_else(|| AppError::Http(StatusCode::NOT_FOUND, "No file available".into()))?;
    let path_str = asset.file_path.ok_or_else(|| AppError::Http(StatusCode::NOT_FOUND, "No file available".into()))?;
    let path = FsPath::new(&path_str);
    if !in_uploads(path) || !path.exists() { return Err(AppError::Http(StatusCode::NOT_FOUND, "File not found".into())); }
    if req.method() == Method::HEAD {
        return Ok(Response::builder().status(StatusCode::OK).body(axum::body::Body::empty()).unwrap());
    }
    let bytes = tokio::fs::read(path).await.map_err(|_| AppError::Http(StatusCode::NOT_FOUND, "File not found".into()))?;
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("download.bin");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

async fn create_source_json(
    State(state): State<AppState>,
    Json(payload): Json<SourceCreate>,
) -> Result<Json<SourceDetailResponse>, AppError> {
    create_source_inner(state, payload, None, None).await
}

async fn create_source(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<SourceDetailResponse>, AppError> {
    // ponytail: minimal multipart parser matching frontend form-data fields.
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().map(|s| s.to_string());
            file_bytes = Some(field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?.to_vec());
        } else {
            let v = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
            fields.insert(name, v);
        }
    }
    let parse_bool = |k: &str| fields.get(k).map(|v| matches!(v.as_str(), "true"|"1"|"yes"|"on")).unwrap_or(false);
    let notebooks = fields.get("notebooks").and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());
    let transformations = fields.get("transformations").and_then(|s| serde_json::from_str::<Vec<String>>(s).ok()).unwrap_or_default();
    let payload = SourceCreate {
        notebook_id: fields.get("notebook_id").cloned(),
        notebooks,
        source_type: fields.get("type").cloned().unwrap_or_default(),
        url: fields.get("url").cloned(),
        file_path: None,
        content: fields.get("content").cloned(),
        title: fields.get("title").cloned(),
        transformations: Some(transformations),
        embed: parse_bool("embed"),
        delete_source: parse_bool("delete_source"),
        async_processing: parse_bool("async_processing"),
    };
    create_source_inner(state, payload, file_bytes, file_name).await
}

fn normalize_notebooks(mut p: SourceCreate) -> Result<SourceCreate, AppError> {
    if p.notebook_id.is_some() && p.notebooks.is_some() {
        return Err(AppError::BadRequest("Cannot specify both notebook_id and notebooks".into()));
    }
    if let Some(nid) = p.notebook_id.take() { p.notebooks = Some(vec![nid]); }
    if p.notebooks.is_none() { p.notebooks = Some(vec![]); }
    Ok(p)
}

async fn create_source_inner(
    state: AppState,
    payload: SourceCreate,
    file_bytes: Option<Vec<u8>>,
    file_name: Option<String>,
) -> Result<Json<SourceDetailResponse>, AppError> {
    let payload = normalize_notebooks(payload)?;
    let notebooks: Vec<Uuid> = payload.notebooks.clone().unwrap_or_default()
        .into_iter().filter_map(|s| Uuid::parse_str(&s).ok()).collect();

    let mut asset: Option<Value> = None;
    let mut full_text: Option<String> = None;
    let mut saved_path: Option<PathBuf> = None;

    match payload.source_type.as_str() {
        "link" => {
            let url = payload.url.clone().ok_or_else(|| AppError::BadRequest("URL is required for link type".into()))?;
            validate_url(&url)?;
            asset = Some(json!({"url": url}));
        }
        "text" => {
            let content = payload.content.clone().ok_or_else(|| AppError::BadRequest("Content is required for text type".into()))?;
            full_text = Some(content);
        }
        "upload" => {
            if let Some(bytes) = file_bytes {
                let name = file_name.unwrap_or_else(|| "upload.bin".into());
                let path = unique_upload_path(&name).await?;
                let mut f = tokio::fs::File::create(&path).await.map_err(|e| AppError::Http(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                f.write_all(&bytes).await.map_err(|e| AppError::Http(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                asset = Some(json!({"file_path": path.to_string_lossy()}));
                saved_path = Some(path);
            } else if let Some(fp) = payload.file_path.clone() {
                let p = FsPath::new(&fp);
                if !in_uploads(p) { return Err(AppError::BadRequest("file_path must be within uploads".into())); }
                asset = Some(json!({"file_path": fp}));
            } else {
                return Err(AppError::BadRequest("File upload or file_path is required for upload type".into()));
            }
        }
        _ => return Err(AppError::BadRequest("Invalid source type".into())),
    }

    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        r#"
        INSERT INTO sources (title, asset, full_text, status, processing_info)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(payload.title.clone().or_else(|| if payload.source_type=="text" { Some("Text source".into()) } else { Some("Processing...".into()) }))
    .bind(asset.clone())
    .bind(full_text.clone())
    .bind(if payload.async_processing { Some("queued") } else { Some("completed") })
    .bind(if payload.async_processing { Some(json!({"async": true, "queued": true})) } else { Some(json!({"async": false})) })
    .fetch_one(&mut *tx).await?;
    let source_id: Uuid = row.get("id");

    for nid in &notebooks {
        sqlx::query("INSERT INTO source_notebooks (source_id, notebook_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(source_id).bind(nid).execute(&mut *tx).await?;
    }

    let mut command_id: Option<Uuid> = None;
    if payload.async_processing {
        // ponytail: queue only; no worker executes process_source yet. Status stays queued for UI polling.
        let input = json!({
            "source_id": source_id.to_string(),
            "content_state": { "url": payload.url, "file_path": asset.as_ref().and_then(|a| a.get("file_path")), "content": payload.content },
            "notebook_ids": notebooks.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            "transformations": payload.transformations.clone().unwrap_or_default(),
            "embed": payload.embed,
        });
        let jid = sqlx::query("INSERT INTO jobs (app_name, command_name, input, status) VALUES ('open_notebook','process_source',$1,'queued') RETURNING id")
            .bind(input).fetch_one(&mut *tx).await?;
        let cid: Uuid = jid.get("id");
        sqlx::query("UPDATE sources SET command_id=$1 WHERE id=$2").bind(cid).bind(source_id).execute(&mut *tx).await?;
        command_id = Some(cid);
    } else if payload.source_type == "upload" {
        // ponytail: sync upload stores file + marks completed; extraction/embedding deferred to processing pipeline.
        let info = json!({"sync": true, "note": "stored; extraction/embedding pending pipeline"});
        sqlx::query("UPDATE sources SET processing_info=$1 WHERE id=$2").bind(info).bind(source_id).execute(&mut *tx).await?;
    }

    tx.commit().await?;

    let mut detail = get_source_detail(&state.pool, source_id).await?;
    if let Some(cid) = command_id { detail.base.command_id = Some(cid.to_string()); detail.base.status = Some("queued".into()); }
    let _ = saved_path; // retained on disk
    Ok(Json(detail))
}

async fn update_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(payload): Json<SourceUpdate>,
) -> Result<Json<SourceDetailResponse>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT title, topics FROM sources WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let title = payload.title.or_else(|| row.try_get("title").ok());
    let topics: Value = payload.topics.map(|t| json!(t)).or_else(|| row.try_get("topics").ok()).unwrap_or_else(|| json!([]));
    sqlx::query("UPDATE sources SET title=$1, topics=$2, updated_at=NOW() WHERE id=$3")
        .bind(title).bind(topics).bind(id).execute(&state.pool).await?;
    Ok(Json(get_source_detail(&state.pool, id).await?))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT asset FROM sources WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    if let Some(asset) = parse_asset(row.try_get("asset").ok()) {
        if let Some(fp) = asset.file_path {
            let p = FsPath::new(&fp);
            if in_uploads(p) { let _ = tokio::fs::remove_file(p).await; }
        }
    }
    sqlx::query("DELETE FROM sources WHERE id=$1").bind(id).execute(&state.pool).await?;
    Ok(Json(json!({"success": true})))
}

async fn retry_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<SourceDetailResponse>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    // ponytail: enqueue a fresh queued job and point source at it; worker execution comes with job pipeline section.
    let existing = sqlx::query("SELECT asset, full_text FROM sources WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let notebooks: Vec<String> = sqlx::query("SELECT notebook_id FROM source_notebooks WHERE source_id=$1").bind(id).fetch_all(&state.pool).await?
        .into_iter().map(|r| r.get::<Uuid,_>("notebook_id").to_string()).collect();
    let input = json!({"source_id": id.to_string(), "content_state": {"asset": existing.try_get::<Option<Value>,_>("asset").ok().flatten(), "content": existing.try_get::<Option<String>,_>("full_text").ok().flatten()}, "notebook_ids": notebooks, "transformations": [], "embed": true});
    let row = sqlx::query("INSERT INTO jobs (app_name, command_name, input, status) VALUES ('open_notebook','process_source',$1,'queued') RETURNING id")
        .bind(input).fetch_one(&state.pool).await?;
    let cid: Uuid = row.get("id");
    sqlx::query("UPDATE sources SET command_id=$1, status='queued', processing_info=$2, updated_at=NOW() WHERE id=$3")
        .bind(cid).bind(json!({"async": true, "queued": true, "retry": true})).bind(id).execute(&state.pool).await?;
    let mut detail = get_source_detail(&state.pool, id).await?;
    detail.base.command_id = Some(cid.to_string());
    detail.base.status = Some("queued".into());
    Ok(Json(detail))
}

async fn list_source_insights(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Vec<SourceInsightResponse>>, AppError> {
    let id = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    let rows = sqlx::query("SELECT id, source_id, insight_type, content, created_at, updated_at FROM source_insights WHERE source_id=$1 ORDER BY created_at DESC")
        .bind(id).fetch_all(&state.pool).await?;
    Ok(Json(rows.into_iter().map(|r| SourceInsightResponse {
        id: r.get::<Uuid,_>("id").to_string(),
        source_id: r.get::<Uuid,_>("source_id").to_string(),
        insight_type: r.get("insight_type"),
        content: r.get("content"),
        created: fmt_opt(r.try_get("created_at").ok()),
        updated: fmt_opt(r.try_get("updated_at").ok()),
    }).collect()))
}

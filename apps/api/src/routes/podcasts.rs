use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Method, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::path::{Path as FsPath, PathBuf};
use uuid::Uuid;

use crate::{errors::AppError, jobs, routes::{episode_profiles, speaker_profiles}, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/podcasts/generate", post(generate_podcast))
        .route("/podcasts/jobs/:job_id", get(get_podcast_job_status))
        .route("/podcasts/episodes", get(list_episodes))
        .route("/podcasts/episodes/:episode_id", get(get_episode).delete(delete_episode))
        .route("/podcasts/episodes/:episode_id/retry", post(retry_episode))
        .route("/podcasts/episodes/:episode_id/audio", get(stream_episode_audio).head(stream_episode_audio))
        .route("/languages", get(list_languages))
}

#[derive(Deserialize)]
pub struct PodcastGenerationRequest {
    pub episode_profile: String,
    pub speaker_profile: String,
    pub episode_name: String,
    pub content: Option<String>,
    pub notebook_id: Option<String>,
    pub briefing_suffix: Option<String>,
}

#[derive(Serialize)]
pub struct PodcastGenerationResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
    pub episode_profile: String,
    pub episode_name: String,
}

#[derive(Serialize)]
pub struct Language {
    pub code: String,
    pub name: String,
}

fn podcasts_dir() -> PathBuf {
    PathBuf::from(std::env::var("PODCASTS_DIR").unwrap_or_else(|_| "./data/podcasts".into()))
}
fn in_podcasts_dir(path: &FsPath) -> bool {
    let root = podcasts_dir().canonicalize().unwrap_or_else(|_| podcasts_dir());
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p.starts_with(root)
}

async fn episode_job_status(pool: &sqlx::PgPool, command_id: Option<Uuid>) -> (Option<String>, Option<String>) {
    let Some(cid) = command_id else { return (None, None); };
    match jobs::status_payload(pool, cid).await {
        Ok(v) => (
            v.get("status").and_then(|s| s.as_str()).map(str::to_string),
            v.get("error_message").and_then(|s| s.as_str()).map(str::to_string),
        ),
        Err(_) => (Some("unknown".into()), None),
    }
}

async fn episode_to_response(state: &AppState, r: &sqlx::postgres::PgRow) -> Value {
    let id: Uuid = r.get("id");
    let command_id: Option<Uuid> = r.try_get("command_id").ok().flatten();
    let (job_status, error_message) = episode_job_status(&state.pool, command_id).await;
    let audio_file: Option<String> = r.try_get("audio_file").ok().flatten();
    let audio_url = audio_file.as_ref().and_then(|f| {
        let p = FsPath::new(f);
        let exists = if p.is_absolute() { p.exists() } else { podcasts_dir().join(p).exists() || in_podcasts_dir(p) };
        if exists { Some(format!("/api/podcasts/episodes/{}/audio", id)) } else { None }
    });
    let created: Option<DateTime<Utc>> = r.try_get("created_at").ok();
    json!({
        "id": id.to_string(),
        "name": r.try_get::<String,_>("name").unwrap_or_default(),
        "episode_profile": r.try_get::<Value,_>("episode_profile").unwrap_or_else(|_| json!({})),
        "speaker_profile": r.try_get::<Value,_>("speaker_profile").unwrap_or_else(|_| json!({})),
        "briefing": r.try_get::<String,_>("briefing").unwrap_or_default(),
        "audio_file": audio_file,
        "audio_url": audio_url,
        "transcript": r.try_get::<Option<Value>,_>("transcript").ok().flatten(),
        "outline": r.try_get::<Option<Value>,_>("outline").ok().flatten(),
        "created": created.map(|t| t.to_rfc3339()),
        "job_status": job_status.or_else(|| if r.try_get::<Option<String>,_>("audio_file").ok().flatten().is_some() { Some("completed".into()) } else { Some("unknown".into()) }),
        "error_message": error_message,
    })
}

async fn fetch_episode_profile_row(pool: &sqlx::PgPool, id_or_name: &str) -> Result<sqlx::postgres::PgRow, AppError> {
    episode_profiles::fetch_by_id_or_name(pool, id_or_name).await?.ok_or_else(|| AppError::BadRequest(format!("Episode profile '{id_or_name}' not found")))
}
async fn fetch_speaker_profile_row(pool: &sqlx::PgPool, id_or_name: &str) -> Result<sqlx::postgres::PgRow, AppError> {
    speaker_profiles::fetch_profile_by_id_or_name(pool, id_or_name).await?.ok_or_else(|| AppError::BadRequest(format!("Speaker profile '{id_or_name}' not found")))
}

async fn generate_podcast(
    State(state): State<AppState>,
    Json(req): Json<PodcastGenerationRequest>,
) -> Result<Json<PodcastGenerationResponse>, AppError> {
    let ep = fetch_episode_profile_row(&state.pool, &req.episode_profile).await?;
    let sp = fetch_speaker_profile_row(&state.pool, &req.speaker_profile).await?;
    let ep_name: String = ep.get("name");
    let sp_name: String = sp.get("name");

    let episode_snapshot = json!({
        "id": ep.get::<Uuid,_>("id").to_string(),
        "name": ep_name,
        "description": ep.try_get::<Option<String>,_>("description").ok().flatten().unwrap_or_default(),
        "speaker_config": ep.try_get::<Option<String>,_>("speaker_config").ok().flatten(),
        "speaker_config_name": Some(sp_name.clone()),
        "outline_llm": ep.try_get::<Option<String>,_>("outline_llm").ok().flatten(),
        "transcript_llm": ep.try_get::<Option<String>,_>("transcript_llm").ok().flatten(),
        "language": ep.try_get::<Option<String>,_>("language").ok().flatten(),
        "default_briefing": ep.try_get::<Option<String>,_>("default_briefing").ok().flatten().unwrap_or_default(),
        "num_segments": ep.try_get::<i32,_>("num_segments").unwrap_or(5),
        "max_tokens": ep.try_get::<Option<i32>,_>("max_tokens").ok().flatten(),
    });
    let speakers_val: Value = sp.try_get("speakers").unwrap_or_else(|_| json!([]));
    let speaker_snapshot = json!({
        "id": sp.get::<Uuid,_>("id").to_string(),
        "name": sp_name,
        "description": sp.try_get::<Option<String>,_>("description").ok().flatten().unwrap_or_default(),
        "voice_model": sp.try_get::<Option<String>,_>("voice_model").ok().flatten(),
        "speakers": speakers_val,
    });
    let mut briefing = ep.try_get::<Option<String>,_>("default_briefing").ok().flatten().unwrap_or_default();
    if let Some(suffix) = &req.briefing_suffix {
        if !suffix.trim().is_empty() {
            if !briefing.is_empty() { briefing.push_str("\n\n"); }
            briefing.push_str(suffix);
        }
    }
    let content = req.content.clone().unwrap_or_default();
    let notebook_id = req.notebook_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    let row = sqlx::query(
        r#"INSERT INTO episodes (name, episode_profile, speaker_profile, briefing, content, notebook_id)
           VALUES ($1,$2,$3,$4,$5,$6) RETURNING id"#,
    )
    .bind(&req.episode_name)
    .bind(&episode_snapshot)
    .bind(&speaker_snapshot)
    .bind(&briefing)
    .bind(&content)
    .bind(notebook_id)
    .fetch_one(&state.pool)
    .await?;
    let episode_id: Uuid = row.get("id");

    let input = json!({
        "episode_id": episode_id.to_string(),
        "episode_profile": req.episode_profile,
        "speaker_profile": req.speaker_profile,
        "episode_name": req.episode_name,
        "notebook_id": req.notebook_id,
        "content": req.content,
        "briefing_suffix": req.briefing_suffix,
    });
    let job_id = jobs::enqueue(&state.pool, "psynapse", "generate_podcast", input).await?;
    sqlx::query("UPDATE episodes SET command_id=$1, updated_at=NOW() WHERE id=$2")
        .bind(job_id).bind(episode_id).execute(&state.pool).await?;

    Ok(Json(PodcastGenerationResponse {
        job_id: job_id.to_string(),
        status: "submitted".into(),
        message: format!("Podcast generation started for episode '{}'", req.episode_name),
        episode_profile: req.episode_profile,
        episode_name: req.episode_name,
    }))
}

async fn get_podcast_job_status(State(state): State<AppState>, Path(job_id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&job_id).map_err(|_| AppError::NotFound)?;
    Ok(Json(jobs::status_payload(&state.pool, id).await?))
}

async fn list_episodes(State(state): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    let rows = sqlx::query("SELECT * FROM episodes ORDER BY created_at DESC").fetch_all(&state.pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let ep = episode_to_response(&state, r).await;
        // ponytail: skip incomplete placeholder-less records? keep all for now.
        out.push(ep);
    }
    Ok(Json(out))
}

async fn get_episode(State(state): State<AppState>, Path(episode_id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&episode_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT * FROM episodes WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    Ok(Json(episode_to_response(&state, &row).await))
}

async fn delete_episode(State(state): State<AppState>, Path(episode_id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&episode_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT audio_file FROM episodes WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    if let Some(af) = row.try_get::<Option<String>, _>("audio_file").ok().flatten() {
        let p = PathBuf::from(&af);
        let path = if p.is_absolute() { p } else { podcasts_dir().join(&af) };
        if in_podcasts_dir(&path) || podcasts_dir().join(&af).starts_with(podcasts_dir()) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    sqlx::query("DELETE FROM episodes WHERE id=$1").bind(id).execute(&state.pool).await?;
    Ok(Json(json!({"message": "Episode deleted successfully"})))
}

async fn retry_episode(State(state): State<AppState>, Path(episode_id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&episode_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT name, content, notebook_id, episode_profile, speaker_profile, briefing FROM episodes WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let input = json!({
        "episode_id": id.to_string(),
        "episode_name": row.try_get::<String,_>("name").unwrap_or_default(),
        "content": row.try_get::<Option<String>,_>("content").ok().flatten(),
        "notebook_id": row.try_get::<Option<Uuid>,_>("notebook_id").ok().flatten().map(|u| u.to_string()),
        "retry": true,
    });
    let job_id = jobs::enqueue(&state.pool, "psynapse", "generate_podcast", input).await?;
    sqlx::query("UPDATE episodes SET command_id=$1, updated_at=NOW() WHERE id=$2")
        .bind(job_id).bind(id).execute(&state.pool).await?;
    Ok(Json(json!({"job_id": job_id.to_string(), "message": "Podcast generation retry started"})))
}

async fn stream_episode_audio(
    State(state): State<AppState>,
    Path(episode_id): Path<String>,
    req: axum::http::Request<Body>,
) -> Result<Response, AppError> {
    let id = Uuid::parse_str(&episode_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT audio_file FROM episodes WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let af: String = row.try_get::<Option<String>, _>("audio_file").ok().flatten()
        .ok_or_else(|| AppError::Http(StatusCode::NOT_FOUND, "Audio file not found".into()))?;
    let path = {
        let p = PathBuf::from(&af);
        if p.is_absolute() { p } else { podcasts_dir().join(&af) }
    };
    let root = podcasts_dir().canonicalize().unwrap_or_else(|_| podcasts_dir());
    let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
    if !canon.starts_with(&root) || !path.exists() {
        return Err(AppError::Http(StatusCode::NOT_FOUND, "Audio file not found".into()));
    }
    // ponytail: containment check + basic serving; full range/streaming parity pending.
    if req.method() == Method::HEAD {
        return Ok(Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap());
    }
    let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::Http(StatusCode::NOT_FOUND, "Audio file not found".into()))?;
    let ct = if af.to_ascii_lowercase().ends_with(".wav") { "audio/wav" } else { "audio/mpeg" };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(bytes))
        .unwrap())
}

async fn list_languages() -> Json<Vec<Language>> {
    // ponytail: static subset; full language support parity pending.
    Json(vec![
        Language { code: "en-US".into(), name: "English (US)".into() },
        Language { code: "en-GB".into(), name: "English (UK)".into() },
        Language { code: "pt-BR".into(), name: "Portuguese (Brazil)".into() },
        Language { code: "es-ES".into(), name: "Spanish (Spain)".into() },
        Language { code: "fr-FR".into(), name: "French".into() },
        Language { code: "de-DE".into(), name: "German".into() },
        Language { code: "it-IT".into(), name: "Italian".into() },
        Language { code: "ja-JP".into(), name: "Japanese".into() },
        Language { code: "ko-KR".into(), name: "Korean".into() },
        Language { code: "zh-CN".into(), name: "Chinese (Simplified)".into() },
    ])
}

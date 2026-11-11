use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, routes::speaker_profiles, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/episode-profiles", get(list_profiles).post(create_profile))
        .route(
            "/episode-profiles/:profile_id",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route("/episode-profiles/:profile_id/duplicate", post(duplicate_profile))
}

#[derive(Serialize, Clone)]
pub struct EpisodeProfileResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub speaker_config: Option<String>,
    pub speaker_config_name: Option<String>,
    pub outline_llm: Option<String>,
    pub transcript_llm: Option<String>,
    pub language: Option<String>,
    pub default_briefing: String,
    pub num_segments: i32,
    pub max_tokens: Option<i32>,
}

#[derive(Deserialize)]
pub struct EpisodeProfileInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub speaker_config: String,
    pub outline_llm: Option<String>,
    pub transcript_llm: Option<String>,
    pub language: Option<String>,
    pub default_briefing: String,
    #[serde(default = "default_segments")]
    pub num_segments: i32,
    pub max_tokens: Option<i32>,
}
fn default_segments() -> i32 { 5 }

async fn resolve_speaker_name(state: &AppState, speaker_config: Option<&str>) -> Option<String> {
    let sc = speaker_config?;
    if sc.trim().is_empty() { return None; }
    let row = speaker_profiles::fetch_profile_by_id_or_name(&state.pool, sc).await.ok().flatten()?;
    row.try_get::<String, _>("name").ok()
}

async fn resolve_speaker_config_input(state: &AppState, value: &str) -> Result<String, AppError> {
    let row = speaker_profiles::fetch_profile_by_id_or_name(&state.pool, value)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("Speaker profile '{value}' not found")))?;
    Ok(row.get::<Uuid, _>("id").to_string())
}

async fn to_resp(state: &AppState, r: &sqlx::postgres::PgRow) -> EpisodeProfileResponse {
    let speaker_config: Option<String> = r.try_get("speaker_config").ok().flatten();
    let speaker_config_name = resolve_speaker_name(state, speaker_config.as_deref()).await;
    EpisodeProfileResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        description: r.try_get::<Option<String>, _>("description").ok().flatten().unwrap_or_default(),
        speaker_config,
        speaker_config_name,
        outline_llm: r.try_get("outline_llm").ok().flatten(),
        transcript_llm: r.try_get("transcript_llm").ok().flatten(),
        language: r.try_get("language").ok().flatten(),
        default_briefing: r.try_get::<Option<String>, _>("default_briefing").ok().flatten().unwrap_or_default(),
        num_segments: r.try_get::<i32, _>("num_segments").unwrap_or(5),
        max_tokens: r.try_get("max_tokens").ok().flatten(),
    }
}

pub async fn fetch_by_id_or_name(pool: &sqlx::PgPool, id_or_name: &str) -> Result<Option<sqlx::postgres::PgRow>, AppError> {
    if let Ok(id) = Uuid::parse_str(id_or_name) {
        if let Some(row) = sqlx::query("SELECT * FROM episode_profiles WHERE id=$1").bind(id).fetch_optional(pool).await? {
            return Ok(Some(row));
        }
    }
    Ok(sqlx::query("SELECT * FROM episode_profiles WHERE name=$1").bind(id_or_name).fetch_optional(pool).await?)
}

async fn list_profiles(State(state): State<AppState>) -> Result<Json<Vec<EpisodeProfileResponse>>, AppError> {
    let rows = sqlx::query("SELECT * FROM episode_profiles ORDER BY name ASC").fetch_all(&state.pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows { out.push(to_resp(&state, r).await); }
    Ok(Json(out))
}

async fn get_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<EpisodeProfileResponse>, AppError> {
    let row = fetch_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_resp(&state, &row).await))
}

async fn create_profile(State(state): State<AppState>, Json(p): Json<EpisodeProfileInput>) -> Result<Json<EpisodeProfileResponse>, AppError> {
    if !(3..=20).contains(&p.num_segments) {
        return Err(AppError::BadRequest("Number of segments must be between 3 and 20".into()));
    }
    let speaker_config = resolve_speaker_config_input(&state, &p.speaker_config).await?;
    let row = sqlx::query(
        r#"INSERT INTO episode_profiles (name, description, speaker_config, outline_llm, transcript_llm, language, default_briefing, num_segments, max_tokens)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
    )
    .bind(&p.name)
    .bind(if p.description.is_empty() { None } else { Some(p.description) })
    .bind(speaker_config)
    .bind(p.outline_llm)
    .bind(p.transcript_llm)
    .bind(p.language)
    .bind(p.default_briefing)
    .bind(p.num_segments)
    .bind(p.max_tokens)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&state, &row).await))
}

async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(p): Json<EpisodeProfileInput>,
) -> Result<Json<EpisodeProfileResponse>, AppError> {
    let cur = fetch_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let id: Uuid = cur.get("id");
    if !(3..=20).contains(&p.num_segments) {
        return Err(AppError::BadRequest("Number of segments must be between 3 and 20".into()));
    }
    let speaker_config = resolve_speaker_config_input(&state, &p.speaker_config).await?;
    let row = sqlx::query(
        r#"UPDATE episode_profiles
           SET name=$1, description=$2, speaker_config=$3, outline_llm=$4, transcript_llm=$5, language=$6, default_briefing=$7, num_segments=$8, max_tokens=$9, updated_at=NOW()
           WHERE id=$10 RETURNING *"#,
    )
    .bind(&p.name)
    .bind(if p.description.is_empty() { None } else { Some(p.description) })
    .bind(speaker_config)
    .bind(p.outline_llm)
    .bind(p.transcript_llm)
    .bind(p.language)
    .bind(p.default_briefing)
    .bind(p.num_segments)
    .bind(p.max_tokens)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&state, &row).await))
}

async fn delete_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<Value>, AppError> {
    let cur = fetch_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let id: Uuid = cur.get("id");
    sqlx::query("DELETE FROM episode_profiles WHERE id=$1").bind(id).execute(&state.pool).await?;
    Ok(Json(json!({"message": "Episode profile deleted successfully"})))
}

async fn duplicate_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<EpisodeProfileResponse>, AppError> {
    let cur = fetch_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let orig = to_resp(&state, &cur).await;
    let copy_name = format!("{} - Copy", orig.name);
    let row = sqlx::query(
        r#"INSERT INTO episode_profiles (name, description, speaker_config, outline_llm, transcript_llm, language, default_briefing, num_segments, max_tokens)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
    )
    .bind(copy_name)
    .bind(if orig.description.is_empty() { None } else { Some(orig.description) })
    .bind(orig.speaker_config)
    .bind(orig.outline_llm)
    .bind(orig.transcript_llm)
    .bind(orig.language)
    .bind(orig.default_briefing)
    .bind(orig.num_segments)
    .bind(orig.max_tokens)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&state, &row).await))
}

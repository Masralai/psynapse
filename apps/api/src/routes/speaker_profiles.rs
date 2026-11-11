use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/speaker-profiles", get(list_profiles).post(create_profile))
        .route(
            "/speaker-profiles/:profile_id",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route("/speaker-profiles/:profile_id/duplicate", post(duplicate_profile))
}

#[derive(Serialize, Clone)]
pub struct SpeakerProfileResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub voice_model: Option<String>,
    pub speakers: Vec<Value>,
}

#[derive(Deserialize)]
pub struct SpeakerProfileInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub voice_model: Option<String>,
    pub speakers: Vec<Value>,
}

fn to_resp(r: &sqlx::postgres::PgRow) -> SpeakerProfileResponse {
    let speakers: Value = r.try_get("speakers").unwrap_or_else(|_| json!([]));
    SpeakerProfileResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        description: r.try_get::<Option<String>, _>("description").ok().flatten().unwrap_or_default(),
        voice_model: r.try_get("voice_model").ok().flatten(),
        speakers: speakers.as_array().cloned().unwrap_or_default(),
    }
}

pub async fn fetch_profile_by_id_or_name(pool: &sqlx::PgPool, id_or_name: &str) -> Result<Option<sqlx::postgres::PgRow>, AppError> {
    if let Ok(id) = Uuid::parse_str(id_or_name) {
        if let Some(row) = sqlx::query("SELECT * FROM speaker_profiles WHERE id=$1").bind(id).fetch_optional(pool).await? {
            return Ok(Some(row));
        }
    }
    Ok(sqlx::query("SELECT * FROM speaker_profiles WHERE name=$1").bind(id_or_name).fetch_optional(pool).await?)
}

async fn list_profiles(State(state): State<AppState>) -> Result<Json<Vec<SpeakerProfileResponse>>, AppError> {
    let rows = sqlx::query("SELECT * FROM speaker_profiles ORDER BY name ASC").fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(to_resp).collect()))
}

async fn get_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<SpeakerProfileResponse>, AppError> {
    let row = fetch_profile_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_resp(&row)))
}

async fn create_profile(State(state): State<AppState>, Json(p): Json<SpeakerProfileInput>) -> Result<Json<SpeakerProfileResponse>, AppError> {
    if p.speakers.is_empty() || p.speakers.len() > 4 {
        return Err(AppError::BadRequest("Must have between 1 and 4 speakers".into()));
    }
    let row = sqlx::query(
        "INSERT INTO speaker_profiles (name, description, voice_model, speakers) VALUES ($1,$2,$3,$4) RETURNING *",
    )
    .bind(&p.name)
    .bind(if p.description.is_empty() { None } else { Some(p.description) })
    .bind(p.voice_model)
    .bind(json!(p.speakers))
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&row)))
}

async fn update_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(p): Json<SpeakerProfileInput>,
) -> Result<Json<SpeakerProfileResponse>, AppError> {
    let cur = fetch_profile_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let id: Uuid = cur.get("id");
    if p.speakers.is_empty() || p.speakers.len() > 4 {
        return Err(AppError::BadRequest("Must have between 1 and 4 speakers".into()));
    }
    let row = sqlx::query(
        "UPDATE speaker_profiles SET name=$1, description=$2, voice_model=$3, speakers=$4, updated_at=NOW() WHERE id=$5 RETURNING *",
    )
    .bind(&p.name)
    .bind(if p.description.is_empty() { None } else { Some(p.description) })
    .bind(p.voice_model)
    .bind(json!(p.speakers))
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&row)))
}

async fn delete_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<Value>, AppError> {
    let cur = fetch_profile_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let id: Uuid = cur.get("id");
    sqlx::query("DELETE FROM speaker_profiles WHERE id=$1").bind(id).execute(&state.pool).await?;
    Ok(Json(json!({"message": "Speaker profile deleted successfully"})))
}

async fn duplicate_profile(State(state): State<AppState>, Path(profile_id): Path<String>) -> Result<Json<SpeakerProfileResponse>, AppError> {
    let cur = fetch_profile_by_id_or_name(&state.pool, &profile_id).await?.ok_or(AppError::NotFound)?;
    let orig = to_resp(&cur);
    let copy_name = format!("{} - Copy", orig.name);
    let row = sqlx::query(
        "INSERT INTO speaker_profiles (name, description, voice_model, speakers) VALUES ($1,$2,$3,$4) RETURNING *",
    )
    .bind(copy_name)
    .bind(if orig.description.is_empty() { None } else { Some(orig.description) })
    .bind(orig.voice_model)
    .bind(json!(orig.speakers))
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(to_resp(&row)))
}

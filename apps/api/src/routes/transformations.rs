use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transformations", get(list_transformations).post(create_transformation))
        .route("/transformations/execute", post(execute_transformation))
        .route(
            "/transformations/default-prompt",
            get(get_default_prompt).put(update_default_prompt),
        )
        .route(
            "/transformations/:id",
            get(get_transformation)
                .put(update_transformation)
                .delete(delete_transformation),
        )
}

#[derive(Serialize)]
pub struct TransformationResponse {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub prompt: String,
    pub apply_default: bool,
    pub model_id: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Deserialize)]
pub struct CreateTransformationRequest {
    pub name: String,
    pub title: String,
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub apply_default: bool,
    pub model_id: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct UpdateTransformationRequest {
    pub name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub apply_default: Option<bool>,
    pub model_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ExecuteTransformationRequest {
    pub transformation_id: String,
    pub input_text: String,
    pub model_id: Option<String>,
}

#[derive(Serialize)]
pub struct ExecuteTransformationResponse {
    pub output: String,
    pub transformation_id: String,
    pub model_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DefaultPromptResponse {
    pub transformation_instructions: String,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }
fn to_resp(r: &sqlx::postgres::PgRow) -> TransformationResponse {
    TransformationResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        title: r.get("title"),
        description: r.get("description"),
        prompt: r.get("prompt"),
        apply_default: r.get("apply_default"),
        model_id: r.try_get::<Option<Uuid>, _>("model_id").ok().flatten().map(|u| u.to_string()),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
    }
}

async fn list_transformations(State(state): State<AppState>) -> Result<Json<Vec<TransformationResponse>>, AppError> {
    let rows = sqlx::query("SELECT * FROM transformations ORDER BY name ASC").fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(to_resp).collect()))
}

async fn create_transformation(
    State(state): State<AppState>,
    Json(p): Json<CreateTransformationRequest>,
) -> Result<Json<TransformationResponse>, AppError> {
    let model_id = p.model_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let row = sqlx::query(
        "INSERT INTO transformations (name,title,description,prompt,apply_default,model_id) VALUES ($1,$2,$3,$4,$5,$6) RETURNING *",
    )
    .bind(p.name).bind(p.title).bind(p.description).bind(p.prompt).bind(p.apply_default).bind(model_id)
    .fetch_one(&state.pool).await?;
    Ok(Json(to_resp(&row)))
}

async fn get_transformation(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<TransformationResponse>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT * FROM transformations WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_resp(&row)))
}

async fn update_transformation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<UpdateTransformationRequest>,
) -> Result<Json<TransformationResponse>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let cur = sqlx::query("SELECT * FROM transformations WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let name = p.name.unwrap_or_else(|| cur.get("name"));
    let title = p.title.unwrap_or_else(|| cur.get("title"));
    let description = p.description.unwrap_or_else(|| cur.get("description"));
    let prompt = p.prompt.unwrap_or_else(|| cur.get("prompt"));
    let apply_default = p.apply_default.unwrap_or_else(|| cur.get("apply_default"));
    let model_id = p.model_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()).or_else(|| cur.try_get("model_id").ok().flatten());
    let row = sqlx::query(
        "UPDATE transformations SET name=$1,title=$2,description=$3,prompt=$4,apply_default=$5,model_id=$6,updated_at=NOW() WHERE id=$7 RETURNING *",
    )
    .bind(name).bind(title).bind(description).bind(prompt).bind(apply_default).bind(model_id).bind(id)
    .fetch_one(&state.pool).await?;
    Ok(Json(to_resp(&row)))
}

async fn delete_transformation(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let res = sqlx::query("DELETE FROM transformations WHERE id=$1").bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"success": true})))
}

async fn get_default_prompt(State(state): State<AppState>) -> Result<Json<DefaultPromptResponse>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key='default_prompts'")
        .fetch_optional(&state.pool).await?;
    let instructions = v.and_then(|x| x.get("transformation_instructions").and_then(|s| s.as_str()).map(str::to_string)).unwrap_or_default();
    Ok(Json(DefaultPromptResponse { transformation_instructions: instructions }))
}

async fn update_default_prompt(
    State(state): State<AppState>,
    Json(p): Json<DefaultPromptResponse>,
) -> Result<Json<DefaultPromptResponse>, AppError> {
    let value = json!({"transformation_instructions": p.transformation_instructions});
    sqlx::query(
        "INSERT INTO settings (key,value,updated_at) VALUES ('default_prompts',$1,NOW()) ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value, updated_at=NOW()",
    )
    .bind(value).execute(&state.pool).await?;
    Ok(Json(p))
}

async fn execute_transformation(
    State(state): State<AppState>,
    Json(p): Json<ExecuteTransformationRequest>,
) -> Result<Json<ExecuteTransformationResponse>, AppError> {
    let id = Uuid::parse_str(&p.transformation_id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT prompt, model_id, title FROM transformations WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let prompt: String = row.get("prompt");
    let title: String = row.try_get("title").unwrap_or_else(|_| "Transformation".into());
    let model_id = p.model_id.or_else(|| row.try_get::<Option<Uuid>, _>("model_id").ok().flatten().map(|u| u.to_string()));

    // ponytail: single-turn chat-completions execution; no graph/provider-specific features.
    let system = "You apply the given transformation prompt to the user's input. Return only the transformed result.";
    let user = format!("Transformation: {title}\n\nInstructions:\n{prompt}\n\nInput:\n{}", p.input_text);
    let output = match crate::providers::chat_completion(&state.pool, model_id.as_deref(), Some("default_transformation_model"), system, &user).await {
        Ok(text) => text,
        Err(e) => {
            // keep endpoint usable if no provider configured yet
            let msg = e.to_string();
            if msg.contains("No model configured") || msg.contains("No base URL") {
                format!("[transformation pending provider pipeline]\n\nPrompt:\n{prompt}\n\nInput:\n{}", p.input_text)
            } else {
                return Err(e);
            }
        }
    };
    Ok(Json(ExecuteTransformationResponse {
        output,
        transformation_id: id.to_string(),
        model_id,
    }))
}

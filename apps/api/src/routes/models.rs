use axum::{
    extract::{Path, Query, State},
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
        .route("/models", get(list_models).post(create_model))
        .route("/models/defaults", get(get_defaults).put(update_defaults))
        .route("/models/providers", get(get_providers))
        .route("/models/discover/:provider", get(discover_models))
        .route("/models/sync", post(sync_all))
        .route("/models/sync/:provider", post(sync_provider))
        .route("/models/count/:provider", get(model_count))
        .route("/models/by-provider/:provider", get(models_by_provider))
        .route("/models/auto-assign", post(auto_assign))
        .route("/models/:id", get(get_model).delete(delete_model))
        .route("/models/:id/test", post(test_model))
}

#[derive(Serialize)]
pub struct ModelResponse {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub credential: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Deserialize)]
pub struct CreateModelRequest {
    pub name: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub credential: Option<String>,
}

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(rename = "type")]
    pub model_type: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct DefaultModelsResponse {
    pub default_chat_model: Option<String>,
    pub default_transformation_model: Option<String>,
    pub large_context_model: Option<String>,
    pub default_text_to_speech_model: Option<String>,
    pub default_speech_to_text_model: Option<String>,
    pub default_embedding_model: Option<String>,
    pub default_tools_model: Option<String>,
}

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }

fn row_to_model(r: &sqlx::postgres::PgRow) -> ModelResponse {
    ModelResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        provider: r.get("provider"),
        model_type: r.get("type"),
        credential: r.try_get::<Option<Uuid>, _>("credential_id").ok().flatten().map(|u| u.to_string()),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
    }
}

async fn list_models(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<Vec<ModelResponse>>, AppError> {
    let rows = if let Some(t) = p.model_type {
        sqlx::query("SELECT * FROM models WHERE type=$1 ORDER BY provider, name")
            .bind(t).fetch_all(&state.pool).await?
    } else {
        sqlx::query("SELECT * FROM models ORDER BY provider, name")
            .fetch_all(&state.pool).await?
    };
    Ok(Json(rows.iter().map(row_to_model).collect()))
}

async fn get_model(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<ModelResponse>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT * FROM models WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    Ok(Json(row_to_model(&row)))
}

async fn create_model(State(state): State<AppState>, Json(p): Json<CreateModelRequest>) -> Result<Json<ModelResponse>, AppError> {
    let valid = ["language", "embedding", "text_to_speech", "speech_to_text"];
    if !valid.contains(&p.model_type.as_str()) {
        return Err(AppError::BadRequest(format!("Invalid model type. Must be one of: {valid:?}")));
    }
    let cred = p.credential.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    if let Some(cid) = cred {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM credentials WHERE id=$1").bind(cid).fetch_optional(&state.pool).await?;
        if exists.is_none() { return Err(AppError::BadRequest("Credential not found".into())); }
    }
    let dup = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM models WHERE lower(provider)=lower($1) AND lower(name)=lower($2) AND lower(type)=lower($3) LIMIT 1",
    ).bind(&p.provider).bind(&p.name).bind(&p.model_type).fetch_optional(&state.pool).await?;
    if dup.is_some() {
        return Err(AppError::BadRequest(format!("Model '{}' already exists for provider '{}' with type '{}'", p.name, p.provider, p.model_type)));
    }
    let row = sqlx::query(
        "INSERT INTO models (name, provider, type, credential_id) VALUES ($1,$2,$3,$4) RETURNING *",
    )
    .bind(&p.name).bind(&p.provider).bind(&p.model_type).bind(cred)
    .fetch_one(&state.pool).await?;
    Ok(Json(row_to_model(&row)))
}

async fn delete_model(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    // clear defaults referencing this model
    if let Ok(Json(mut defaults)) = get_defaults(State(state.clone())).await {
        let sid = id.to_string();
        let mut changed = false;
        macro_rules! clr { ($f:ident) => { if defaults.$f.as_deref() == Some(&sid) { defaults.$f = None; changed = true; } }; }
        clr!(default_chat_model); clr!(default_transformation_model); clr!(large_context_model);
        clr!(default_text_to_speech_model); clr!(default_speech_to_text_model); clr!(default_embedding_model); clr!(default_tools_model);
        if changed {
            let _ = sqlx::query("INSERT INTO settings (key,value,updated_at) VALUES ('model_defaults',$1,NOW()) ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value, updated_at=NOW()")
                .bind(json!(defaults)).execute(&state.pool).await;
        }
    }
    let res = sqlx::query("DELETE FROM models WHERE id=$1").bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"message":"Model deleted successfully"})))
}

async fn get_defaults(State(state): State<AppState>) -> Result<Json<DefaultModelsResponse>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key='model_defaults'")
        .fetch_optional(&state.pool).await?;
    let d = v.and_then(|x| serde_json::from_value::<DefaultModelsResponse>(x).ok()).unwrap_or_default();
    Ok(Json(d))
}

async fn update_defaults(State(state): State<AppState>, Json(payload): Json<Value>) -> Result<Json<DefaultModelsResponse>, AppError> {
    let mut current = get_defaults(State(state.clone())).await?.0;
    if let Some(o) = payload.as_object() {
        let req = ["default_chat_model","default_embedding_model"];
        for k in req {
            if o.contains_key(k) && o.get(k).map(|v| v.is_null()).unwrap_or(false) {
                return Err(AppError::BadRequest(format!("{k} is required and cannot be cleared, only reassigned")));
            }
        }
        macro_rules! setf { ($f:ident, $k:expr) => { if o.contains_key($k) { current.$f = o.get($k).and_then(|v| v.as_str()).map(str::to_string); } }; }
        setf!(default_chat_model,"default_chat_model");
        setf!(default_transformation_model,"default_transformation_model");
        setf!(large_context_model,"large_context_model");
        setf!(default_text_to_speech_model,"default_text_to_speech_model");
        setf!(default_speech_to_text_model,"default_speech_to_text_model");
        setf!(default_embedding_model,"default_embedding_model");
        setf!(default_tools_model,"default_tools_model");
    }
    sqlx::query("INSERT INTO settings (key,value,updated_at) VALUES ('model_defaults',$1,NOW()) ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value, updated_at=NOW()")
        .bind(json!(current)).execute(&state.pool).await?;
    Ok(Json(current))
}

fn provider_supported_types(provider: &str) -> Vec<String> {
    // ponytail: static capability map; refine with real provider probing/registry later.
    match provider {
        "openai" => vec!["language","embedding","text_to_speech","speech_to_text"],
        "anthropic" | "groq" | "mistral" | "deepseek" | "xai" | "openrouter" | "dashscope" | "minimax" | "cohere" => vec!["language"],
        "google" => vec!["language","embedding","text_to_speech","speech_to_text"],
        "voyage" => vec!["embedding"],
        "elevenlabs" => vec!["text_to_speech","speech_to_text"],
        "deepgram" => vec!["speech_to_text"],
        "ollama" | "openai_compatible" | "azure" | "vertex" | "omlx" | "anthropic_compatible" => vec!["language","embedding","text_to_speech","speech_to_text"],
        _ => vec!["language"],
    }.into_iter().map(str::to_string).collect()
}

async fn get_providers(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let all = vec![
        "openai","anthropic","google","groq","mistral","deepseek","xai","openrouter","voyage","elevenlabs",
        "azure","ollama","openai_compatible","dashscope","minimax","vertex","anthropic_compatible","novita",
        "siliconflow","zai","ppq","cohere","deepgram","omlx"
    ];
    let mut available = vec![];
    let mut unavailable = vec![];
    let mut supported_types = serde_json::Map::new();
    for p in &all {
        let has_cred = sqlx::query_scalar::<_, i64>("SELECT 1 FROM credentials WHERE lower(provider)=lower($1) LIMIT 1")
            .bind(p).fetch_optional(&state.pool).await?.is_some();
        let has_env = std::env::var(match *p {
            "openai"=>"OPENAI_API_KEY","anthropic"=>"ANTHROPIC_API_KEY","google"=>"GOOGLE_API_KEY","groq"=>"GROQ_API_KEY",
            "mistral"=>"MISTRAL_API_KEY","deepseek"=>"DEEPSEEK_API_KEY","xai"=>"XAI_API_KEY","openrouter"=>"OPENROUTER_API_KEY",
            "voyage"=>"VOYAGE_API_KEY","elevenlabs"=>"ELEVENLABS_API_KEY","deepgram"=>"DEEPGRAM_API_KEY","ollama"=>"OLLAMA_API_BASE",
            "omlx"=>"OMLX_API_BASE","dashscope"=>"DASHSCOPE_API_KEY","minimax"=>"MINIMAX_API_KEY","novita"=>"NOVITA_API_KEY",
            "siliconflow"=>"SILICONFLOW_API_KEY","zai"=>"ZAI_API_KEY","ppq"=>"PPQ_API_KEY","cohere"=>"COHERE_API_KEY",
            "azure"=>"AZURE_OPENAI_API_KEY","vertex"=>"VERTEX_PROJECT","openai_compatible"=>"OPENAI_COMPATIBLE_BASE_URL",
            "anthropic_compatible"=>"ANTHROPIC_COMPATIBLE_API_KEY", _=>"__NONE__"
        }).ok().filter(|s| !s.trim().is_empty()).is_some()
        || (*p == "google" && std::env::var("GEMINI_API_KEY").ok().filter(|s| !s.trim().is_empty()).is_some());
        if has_cred || has_env {
            available.push((*p).to_string());
            supported_types.insert((*p).to_string(), json!(provider_supported_types(p)));
        } else {
            unavailable.push((*p).to_string());
        }
    }
    Ok(Json(json!({"available": available, "unavailable": unavailable, "supported_types": supported_types})))
}

async fn discover_models(Path(_provider): Path<String>) -> Json<Value> {
    // ponytail: provider discovery requires external API calls; return empty until provider pipeline lands.
    Json(json!([]))
}
async fn sync_provider(Path(provider): Path<String>) -> Json<Value> {
    Json(json!({"provider": provider, "discovered": 0, "new": 0, "existing": 0}))
}
async fn sync_all() -> Json<Value> {
    Json(json!({"results": {}, "total_discovered": 0, "total_new": 0}))
}
async fn model_count(State(state): State<AppState>, Path(provider): Path<String>) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query("SELECT type, COUNT(*)::bigint as c FROM models WHERE lower(provider)=lower($1) GROUP BY type")
        .bind(&provider).fetch_all(&state.pool).await?;
    let mut counts = serde_json::Map::new();
    let mut total = 0i64;
    for r in rows { let c: i64 = r.get("c"); total += c; counts.insert(r.get::<String,_>("type"), json!(c)); }
    Ok(Json(json!({"provider": provider, "counts": counts, "total": total})))
}
async fn models_by_provider(State(state): State<AppState>, Path(provider): Path<String>) -> Result<Json<Vec<ModelResponse>>, AppError> {
    let rows = sqlx::query("SELECT * FROM models WHERE lower(provider)=lower($1) ORDER BY name")
        .bind(provider).fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(row_to_model).collect()))
}
async fn auto_assign(State(_state): State<AppState>) -> Json<Value> {
    // ponytail: needs ranking/availability logic; placeholder to satisfy API surface.
    Json(json!({"assigned": {}, "skipped": [], "missing": [
        "default_chat_model","default_embedding_model","default_transformation_model","large_context_model",
        "default_text_to_speech_model","default_speech_to_text_model","default_tools_model"
    ]}))
}
async fn test_model(State(_state): State<AppState>, Path(_id): Path<String>) -> Json<Value> {
    Json(json!({"success": false, "message": "Model testing requires provider pipeline (not yet configured)"}))
}

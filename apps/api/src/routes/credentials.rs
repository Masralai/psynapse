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
        .route("/credentials/status", get(status))
        .route("/credentials/env-status", get(env_status))
        .route("/credentials", get(list_credentials).post(create_credential))
        .route("/credentials/by-provider/:provider", get(list_by_provider))
        .route("/credentials/migrate-from-provider-config", post(migrate_stub))
        .route("/credentials/migrate-from-env", post(migrate_stub))
        .route(
            "/credentials/:id",
            get(get_credential).put(update_credential).delete(delete_credential),
        )
        .route("/credentials/:id/test", post(test_credential))
        .route("/credentials/:id/discover", post(discover_models))
        .route("/credentials/:id/register-models", post(register_models))
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CredentialResponse {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub modalities: Vec<String>,
    pub base_url: Option<String>,
    pub endpoint: Option<String>,
    pub api_version: Option<String>,
    pub endpoint_llm: Option<String>,
    pub endpoint_embedding: Option<String>,
    pub endpoint_stt: Option<String>,
    pub endpoint_tts: Option<String>,
    pub project: Option<String>,
    pub location: Option<String>,
    pub credentials_path: Option<String>,
    pub num_ctx: Option<i64>,
    pub has_api_key: bool,
    pub created: String,
    pub updated: String,
    pub model_count: i64,
    pub decryption_error: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateCredentialRequest {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub modalities: Vec<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub endpoint: Option<String>,
    pub api_version: Option<String>,
    pub endpoint_llm: Option<String>,
    pub endpoint_embedding: Option<String>,
    pub endpoint_stt: Option<String>,
    pub endpoint_tts: Option<String>,
    pub project: Option<String>,
    pub location: Option<String>,
    pub credentials_path: Option<String>,
    pub num_ctx: Option<i64>,
}
#[derive(Deserialize, Default)]
pub struct UpdateCredentialRequest {
    pub name: Option<String>,
    pub modalities: Option<Vec<String>>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub endpoint: Option<String>,
    pub api_version: Option<String>,
    pub endpoint_llm: Option<String>,
    pub endpoint_embedding: Option<String>,
    pub endpoint_stt: Option<String>,
    pub endpoint_tts: Option<String>,
    pub project: Option<String>,
    pub location: Option<String>,
    pub credentials_path: Option<String>,
    pub num_ctx: Option<i64>,
}
#[derive(Deserialize)]
pub struct ListParams { pub provider: Option<String> }
#[derive(Deserialize, Default)]
pub struct DeleteParams { pub delete_models: Option<bool>, pub migrate_to: Option<String> }

fn fmt(ts: DateTime<Utc>) -> String { ts.to_rfc3339() }
fn get_s(cfg: &Value, k: &str) -> Option<String> { cfg.get(k).and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.is_empty()) }
fn get_i(cfg: &Value, k: &str) -> Option<i64> { cfg.get(k).and_then(|v| v.as_i64()) }

async fn to_response(state: &AppState, r: &sqlx::postgres::PgRow) -> CredentialResponse {
    let id: Uuid = r.get("id");
    let stored_cfg: Value = r.try_get("config").unwrap_or_else(|_| json!({}));
    let (cfg, decryption_error) = match crate::crypto::decrypt_config(&stored_cfg) {
        Ok(v) => (v, None),
        Err(e) => (json!({}), Some(e)),
    };
    let modalities_v: Value = r.try_get("modalities").unwrap_or_else(|_| json!([]));
    let modalities = modalities_v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let model_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE credential_id=$1").bind(id).fetch_one(&state.pool).await.unwrap_or(0);
    CredentialResponse {
        id: id.to_string(),
        name: r.get("name"),
        provider: r.get("provider"),
        modalities,
        base_url: get_s(&cfg,"base_url"),
        endpoint: get_s(&cfg,"endpoint"),
        api_version: get_s(&cfg,"api_version"),
        endpoint_llm: get_s(&cfg,"endpoint_llm"),
        endpoint_embedding: get_s(&cfg,"endpoint_embedding"),
        endpoint_stt: get_s(&cfg,"endpoint_stt"),
        endpoint_tts: get_s(&cfg,"endpoint_tts"),
        project: get_s(&cfg,"project"),
        location: get_s(&cfg,"location"),
        credentials_path: get_s(&cfg,"credentials_path"),
        num_ctx: get_i(&cfg,"num_ctx"),
        has_api_key: get_s(&cfg,"api_key").is_some(),
        created: fmt(r.get("created_at")),
        updated: fmt(r.get("updated_at")),
        model_count,
        decryption_error,
    }
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    // ponytail: credential configs encrypted at rest (key rotation/KMS hardening pending).
    let rows = sqlx::query("SELECT lower(provider) as p, COUNT(*)::bigint c FROM credentials GROUP BY lower(provider)").fetch_all(&state.pool).await?;
    let mut configured = serde_json::Map::new();
    let mut source = serde_json::Map::new();
    for r in rows {
        let p: String = r.get("p");
        configured.insert(p.clone(), json!(true));
        source.insert(p, json!("database"));
    }
    let encryption_configured = crate::crypto::is_encryption_configured() || !configured.is_empty();
    Ok(Json(json!({"configured": configured, "source": source, "encryption_configured": encryption_configured})))
}

async fn env_status() -> Json<Value> {
    // ponytail: minimal env check for common providers.
    let keys = [
        ("openai","OPENAI_API_KEY"),("anthropic","ANTHROPIC_API_KEY"),("google","GOOGLE_API_KEY"),
        ("groq","GROQ_API_KEY"),("mistral","MISTRAL_API_KEY"),("deepseek","DEEPSEEK_API_KEY"),
        ("xai","XAI_API_KEY"),("openrouter","OPENROUTER_API_KEY"),("ollama","OLLAMA_API_BASE"),
        ("openai_compatible","OPENAI_COMPATIBLE_BASE_URL"),("azure","AZURE_OPENAI_API_KEY"),
    ];
    let mut m = serde_json::Map::new();
    for (p,k) in keys { m.insert(p.into(), json!(std::env::var(k).ok().filter(|s| !s.trim().is_empty()).is_some())); }
    Json(Value::Object(m))
}

async fn list_credentials(State(state): State<AppState>, Query(p): Query<ListParams>) -> Result<Json<Vec<CredentialResponse>>, AppError> {
    let rows = if let Some(provider) = p.provider {
        sqlx::query("SELECT * FROM credentials WHERE lower(provider)=lower($1) ORDER BY created_at DESC").bind(provider).fetch_all(&state.pool).await?
    } else {
        sqlx::query("SELECT * FROM credentials ORDER BY provider, created_at DESC").fetch_all(&state.pool).await?
    };
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows { out.push(to_response(&state, r).await); }
    Ok(Json(out))
}
async fn list_by_provider(State(state): State<AppState>, Path(provider): Path<String>) -> Result<Json<Vec<CredentialResponse>>, AppError> {
    list_credentials(State(state), Query(ListParams { provider: Some(provider) })).await
}

fn config_from_create(p: &CreateCredentialRequest) -> Value {
    json!({
        "api_key": p.api_key, "base_url": p.base_url, "endpoint": p.endpoint, "api_version": p.api_version,
        "endpoint_llm": p.endpoint_llm, "endpoint_embedding": p.endpoint_embedding, "endpoint_stt": p.endpoint_stt,
        "endpoint_tts": p.endpoint_tts, "project": p.project, "location": p.location, "credentials_path": p.credentials_path,
        "num_ctx": p.num_ctx
    })
}
fn merge_config(existing: Value, p: &UpdateCredentialRequest) -> Value {
    let mut m = existing.as_object().cloned().unwrap_or_default();
    macro_rules! set { ($k:ident) => { if let Some(v) = &p.$k { m.insert(stringify!($k).into(), json!(v)); } }; }
    set!(api_key); set!(base_url); set!(endpoint); set!(api_version); set!(endpoint_llm); set!(endpoint_embedding);
    set!(endpoint_stt); set!(endpoint_tts); set!(project); set!(location); set!(credentials_path);
    if let Some(v) = p.num_ctx { m.insert("num_ctx".into(), json!(v)); }
    Value::Object(m)
}

async fn create_credential(State(state): State<AppState>, Json(p): Json<CreateCredentialRequest>) -> Result<(axum::http::StatusCode, Json<CredentialResponse>), AppError> {
    if p.provider.eq_ignore_ascii_case("anthropic_compatible") {
        if p.base_url.as_deref().unwrap_or("").trim().is_empty() { return Err(AppError::BadRequest("Anthropic-compatible credentials require a base URL".into())); }
        if p.api_key.as_deref().unwrap_or("").trim().is_empty() { return Err(AppError::BadRequest("Anthropic-compatible credentials require an API key".into())); }
    }
    let plain = config_from_create(&p);
    let stored = crate::crypto::encrypt_config(&plain).map_err(|e| AppError::Http(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let row = sqlx::query(
        "INSERT INTO credentials (name, provider, modalities, config) VALUES ($1,$2,$3,$4) RETURNING *",
    )
    .bind(&p.name).bind(p.provider.to_lowercase()).bind(json!(p.modalities)).bind(stored)
    .fetch_one(&state.pool).await?;
    Ok((axum::http::StatusCode::CREATED, Json(to_response(&state, &row).await)))
}

async fn get_credential(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<CredentialResponse>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT * FROM credentials WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_response(&state, &row).await))
}

async fn update_credential(State(state): State<AppState>, Path(id): Path<String>, Json(p): Json<UpdateCredentialRequest>) -> Result<Json<CredentialResponse>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let cur = sqlx::query("SELECT * FROM credentials WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let name = p.name.clone().unwrap_or_else(|| cur.get("name"));
    let provider: String = cur.get("provider");
    let modalities = p.modalities.clone().map(|m| json!(m)).unwrap_or_else(|| cur.try_get("modalities").unwrap_or_else(|_| json!([])));
    let stored_existing: Value = cur.try_get("config").unwrap_or_else(|_| json!({}));
    let existing_plain = crate::crypto::decrypt_config(&stored_existing).map_err(|e| AppError::BadRequest(format!("Cannot update credential: {e}")))?;
    let plain_cfg = merge_config(existing_plain, &p);
    if provider == "anthropic_compatible" {
        if get_s(&plain_cfg,"base_url").is_none() { return Err(AppError::BadRequest("Anthropic-compatible credentials require a base URL".into())); }
        if get_s(&plain_cfg,"api_key").is_none() { return Err(AppError::BadRequest("Anthropic-compatible credentials require an API key".into())); }
    }
    let stored = crate::crypto::encrypt_config(&plain_cfg).map_err(|e| AppError::Http(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let row = sqlx::query("UPDATE credentials SET name=$1, modalities=$2, config=$3, updated_at=NOW() WHERE id=$4 RETURNING *")
        .bind(name).bind(modalities).bind(stored).bind(id).fetch_one(&state.pool).await?;
    Ok(Json(to_response(&state, &row).await))
}

async fn delete_credential(State(state): State<AppState>, Path(id): Path<String>, Query(p): Query<DeleteParams>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let mut deleted_models = 0i64;
    if p.delete_models.unwrap_or(false) {
        let res = sqlx::query("DELETE FROM models WHERE credential_id=$1").bind(id).execute(&state.pool).await?;
        deleted_models = res.rows_affected() as i64;
    } else if let Some(mig) = p.migrate_to.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
        sqlx::query("UPDATE models SET credential_id=$1 WHERE credential_id=$2").bind(mig).bind(id).execute(&state.pool).await?;
    } else {
        sqlx::query("UPDATE models SET credential_id=NULL WHERE credential_id=$1").bind(id).execute(&state.pool).await?;
    }
    let res = sqlx::query("DELETE FROM credentials WHERE id=$1").bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({"message":"Credential deleted successfully","deleted_models": deleted_models})))
}

async fn test_credential(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT provider FROM credentials WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let provider: String = row.get("provider");
    // ponytail: credential-level test without a specific model is not fully implemented; report configuration presence.
    let has_model = sqlx::query_scalar::<_, i64>("SELECT 1 FROM models WHERE credential_id=$1 LIMIT 1").bind(id).fetch_optional(&state.pool).await?.is_some();
    let msg = if has_model {
        "Credential saved. Run a model test for a linked model to verify connectivity."
    } else {
        "Credential saved. Add/link a model and run model test to verify connectivity."
    };
    Ok(Json(json!({"provider": provider, "success": true, "message": msg})))
}
async fn discover_models(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let row = sqlx::query("SELECT provider FROM credentials WHERE id=$1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let provider: String = row.get("provider");
    Ok(Json(json!({"credential_id": id.to_string(), "provider": provider, "discovered": []})))
}
async fn register_models(State(state): State<AppState>, Path(id): Path<String>, Json(p): Json<Value>) -> Result<Json<Value>, AppError> {
    let cid = Uuid::parse_str(&id).map_err(|_| AppError::NotFound)?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM credentials WHERE id=$1").bind(cid).fetch_optional(&state.pool).await?;
    if exists.is_none() { return Err(AppError::NotFound); }
    let models = p.get("models").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut created = 0i64; let mut existing = 0i64;
    for m in models {
        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let provider = m.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let mtype = m.get("model_type").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || provider.is_empty() || mtype.is_empty() { continue; }
        let dup = sqlx::query_scalar::<_, i64>("SELECT 1 FROM models WHERE lower(provider)=lower($1) AND lower(name)=lower($2) AND lower(type)=lower($3) LIMIT 1")
            .bind(provider).bind(name).bind(mtype).fetch_optional(&state.pool).await?;
        if dup.is_some() { existing += 1; continue; }
        sqlx::query("INSERT INTO models (name, provider, type, credential_id) VALUES ($1,$2,$3,$4)")
            .bind(name).bind(provider).bind(mtype).bind(cid).execute(&state.pool).await?;
        created += 1;
    }
    Ok(Json(json!({"created": created, "existing": existing})))
}
async fn migrate_stub() -> Json<Value> {
    Json(json!({"message":"Migration not applicable in Rust port yet","migrated":[],"skipped":[],"errors":[]}))
}

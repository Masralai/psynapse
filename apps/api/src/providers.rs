use reqwest::Client;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::AppError;

// ponytail: minimal OpenAI-compatible chat/embedding execution, basic model/credential/env resolution,
// and no tool-calls/provider-specific feature coverage yet.
pub async fn chat_completion(
    pool: &PgPool,
    model_id: Option<&str>,
    default_slot: Option<&str>,
    system: &str,
    user: &str,
) -> Result<String, AppError> {
    let resolved = resolve_chat_model(pool, model_id, default_slot).await?;
    let url = format!("{}/chat/completions", resolved.base_url.trim_end_matches('/'));
    let body = json!({
        "model": resolved.model_name,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2
    });
    // ponytail: minimal OpenAI-compatible payload; provider-specific params omitted.

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Http(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;

    let mut req = client.post(&url).json(&body);
    if let Some(key) = &resolved.api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| {
        AppError::Http(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Provider request failed: {e}"),
        )
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| {
        AppError::Http(axum::http::StatusCode::BAD_GATEWAY, e.to_string())
    })?;
    if !status.is_success() {
        return Err(AppError::Http(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Provider error ({}): {}", status.as_u16(), truncate(&text, 300)),
        ));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        AppError::Http(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Invalid provider response: {e}"),
        )
    })?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|x| x.as_str())
        .or_else(|| v.pointer("/choices/0/text").and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    if content.trim().is_empty() {
        // ponytail: treat empty content as provider/config issue rather than silent success.
        return Err(AppError::Http(
            axum::http::StatusCode::BAD_GATEWAY,
            "Provider returned empty response".into(),
        ));
    }
    Ok(content)
}

pub async fn embed_texts(
    pool: &PgPool,
    model_id: Option<&str>,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    let resolved = resolve_embedding_model(pool, model_id).await?;
    let url = format!("{}/embeddings", resolved.base_url.trim_end_matches('/'));
    let input: Vec<String> = texts.into_iter().map(|t| t.chars().take(8000).collect()).collect();
    let body = json!({ "model": resolved.model_name, "input": input });
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Http(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let mut req = client.post(&url).json(&body);
    if let Some(key) = &resolved.api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| {
        AppError::Http(axum::http::StatusCode::BAD_GATEWAY, format!("Provider request failed: {e}"))
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| AppError::Http(axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    if !status.is_success() {
        return Err(AppError::Http(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Provider error ({}): {}", status.as_u16(), truncate(&text, 300)),
        ));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        AppError::Http(axum::http::StatusCode::BAD_GATEWAY, format!("Invalid provider response: {e}"))
    })?;
    let mut items = v.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    items.sort_by_key(|x| x.get("index").and_then(|i| i.as_i64()).unwrap_or(0));
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let emb = item.get("embedding").and_then(|e| e.as_array()).ok_or_else(|| {
            AppError::Http(axum::http::StatusCode::BAD_GATEWAY, "Invalid embedding response".into())
        })?;
        let vec: Vec<f32> = emb.iter().filter_map(|n| n.as_f64().map(|f| f as f32)).collect();
        out.push(vec);
    }
    if out.is_empty() {
        return Err(AppError::Http(axum::http::StatusCode::BAD_GATEWAY, "Provider returned no embeddings".into()));
    }
    Ok(out)
}

pub fn to_pgvector(v: &[f32]) -> String {
    let inner = v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",");
    format!("[{}]", inner)
}

pub fn chunk_text(text: &str, chunk_chars: usize, overlap: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= chunk_chars {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let end = (i + chunk_chars).min(chars.len());
        let chunk: String = chars[i..end].iter().collect();
        if !chunk.trim().is_empty() {
            out.push(chunk);
        }
        if end == chars.len() {
            break;
        }
        i = end.saturating_sub(overlap.min(chunk_chars.saturating_sub(1)));
        if i == 0 && end == chars.len() { break; }
        if i >= end { i = end; }
    }
    out
}

struct ResolvedModel {
    model_name: String,
    api_key: Option<String>,
    base_url: String,
    #[allow(dead_code)]
    provider: String,
}

async fn resolve_chat_model(
    pool: &PgPool,
    model_id: Option<&str>,
    default_slot: Option<&str>,
) -> Result<ResolvedModel, AppError> {
    resolve_model(pool, model_id, default_slot, "language").await
}

async fn resolve_embedding_model(pool: &PgPool, model_id: Option<&str>) -> Result<ResolvedModel, AppError> {
    resolve_model(pool, model_id, Some("default_embedding_model"), "embedding").await
}

async fn resolve_model(
    pool: &PgPool,
    model_id: Option<&str>,
    default_slot: Option<&str>,
    expected_type: &str,
) -> Result<ResolvedModel, AppError> {
    let mut mid = model_id
        .and_then(|s| if s.trim().is_empty() { None } else { Some(s.trim().to_string()) });

    if mid.is_none() {
        if let Some(slot) = default_slot {
            mid = get_default_model_id(pool, slot).await?;
            if mid.is_none() && expected_type == "language" && slot != "default_chat_model" {
                mid = get_default_model_id(pool, "default_chat_model").await?;
            }
        }
    }
    let mid = mid.ok_or_else(|| {
        AppError::BadRequest(
            if expected_type == "embedding" {
                "No embedding model configured. Set a default embedding model.".into()
            } else {
                "No model configured. Set a default model or provide model_override.".into()
            },
        )
    })?;
    let uuid = Uuid::parse_str(&mid).map_err(|_| AppError::BadRequest("Invalid model id".into()))?;

    let row = sqlx::query(
        "SELECT m.name, m.provider, m.type, c.config as cred_config
         FROM models m
         LEFT JOIN credentials c ON c.id = m.credential_id
         WHERE m.id = $1",
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Configured model not found".into()))?;

    let provider: String = row.get("provider");
    let model_name: String = row.get("name");
    let mtype: String = row.get("type");
    if mtype != expected_type {
        return Err(AppError::BadRequest(format!("Selected model is not a {expected_type} model")));
    }
    let cred_config_raw: Option<Value> = row.try_get("cred_config").ok().flatten();
    let cred_config = match cred_config_raw {
        Some(v) => Some(crate::crypto::decrypt_config(&v).map_err(|e| {
            AppError::BadRequest(format!("Credential decryption failed: {e}"))
        })?),
        None => None,
    };

    let (api_key, base_url) = resolve_provider_config(&provider, cred_config.as_ref());
    let base_url = base_url.ok_or_else(|| {
        AppError::BadRequest(format!(
            "No base URL/API configuration for provider '{provider}'. Configure a credential or environment variable."
        ))
    })?;

    Ok(ResolvedModel { model_name, api_key, base_url, provider })
}

async fn get_default_model_id(pool: &PgPool, slot: &str) -> Result<Option<String>, AppError> {
    let v = sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key='model_defaults'")
        .fetch_optional(pool)
        .await?;
    Ok(v.and_then(|x| x.get(slot).and_then(|s| s.as_str()).map(str::to_string))
        .filter(|s| !s.trim().is_empty()))
}

fn cfg_str(cfg: Option<&Value>, key: &str) -> Option<String> {
    cfg.and_then(|v| v.get(key))
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn resolve_provider_config(provider: &str, cfg: Option<&Value>) -> (Option<String>, Option<String>) {
    let p = provider.to_ascii_lowercase();
    let api_key = cfg_str(cfg, "api_key").or_else(|| match p.as_str() {
        "openai" => env_nonempty("OPENAI_API_KEY"),
        "anthropic" => env_nonempty("ANTHROPIC_API_KEY"),
        "groq" => env_nonempty("GROQ_API_KEY"),
        "mistral" => env_nonempty("MISTRAL_API_KEY"),
        "deepseek" => env_nonempty("DEEPSEEK_API_KEY"),
        "xai" => env_nonempty("XAI_API_KEY"),
        "openrouter" => env_nonempty("OPENROUTER_API_KEY"),
        "google" => env_nonempty("GOOGLE_API_KEY").or_else(|| env_nonempty("GEMINI_API_KEY")),
        "dashscope" => env_nonempty("DASHSCOPE_API_KEY"),
        "openai_compatible" => env_nonempty("OPENAI_COMPATIBLE_API_KEY"),
        "azure" => env_nonempty("AZURE_OPENAI_API_KEY"),
        "anthropic_compatible" => env_nonempty("ANTHROPIC_COMPATIBLE_API_KEY"),
        "voyage" => env_nonempty("VOYAGE_API_KEY"),
        _ => None,
    });

    let base_url = cfg_str(cfg, "base_url")
        .or_else(|| cfg_str(cfg, "endpoint_llm"))
        .or_else(|| cfg_str(cfg, "endpoint"))
        .or_else(|| match p.as_str() {
            "openai" => Some(env_nonempty("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com/v1".into())),
            "groq" => Some("https://api.groq.com/openai/v1".into()),
            "mistral" => Some("https://api.mistral.ai/v1".into()),
            "deepseek" => Some("https://api.deepseek.com/v1".into()),
            "xai" => Some("https://api.x.ai/v1".into()),
            "openrouter" => Some("https://openrouter.ai/api/v1".into()),
            "dashscope" => Some("https://dashscope.aliyuncs.com/compatible-mode/v1".into()),
            "ollama" => Some(env_nonempty("OLLAMA_API_BASE").unwrap_or_else(|| "http://localhost:11434/v1".into())),
            "omlx" => Some(env_nonempty("OMLX_API_BASE").unwrap_or_else(|| "http://localhost:8000/v1".into())),
            "openai_compatible" => env_nonempty("OPENAI_COMPATIBLE_BASE_URL_LLM").or_else(|| env_nonempty("OPENAI_COMPATIBLE_BASE_URL")),
            "anthropic_compatible" => env_nonempty("ANTHROPIC_COMPATIBLE_BASE_URL"),
            "azure" => env_nonempty("AZURE_OPENAI_ENDPOINT"),
            "voyage" => Some("https://api.voyageai.com/v1".into()),
            _ => None,
        });

    // ponytail: azure/anthropic-native endpoints may need different paths/auth; treated as best-effort OpenAI-compatible here.
    (api_key, base_url)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

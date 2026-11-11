use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{errors::AppError, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(get_settings).put(update_settings))
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SettingsResponse {
    pub default_content_processing_engine_doc: Option<String>,
    pub default_content_processing_engine_url: Option<String>,
    pub default_embedding_option: Option<String>,
    pub auto_delete_files: Option<String>,
    pub docling_ocr: Option<bool>,
    pub docling_formulas: Option<bool>,
    pub docling_vision: Option<bool>,
    pub youtube_preferred_languages: Option<Vec<String>>,
}

impl SettingsResponse {
    fn defaults() -> Self {
        Self {
            default_content_processing_engine_doc: Some("auto".into()),
            default_content_processing_engine_url: Some("auto".into()),
            default_embedding_option: Some("ask".into()),
            auto_delete_files: Some("yes".into()),
            docling_ocr: Some(true),
            docling_formulas: Some(false),
            docling_vision: Some(false),
            youtube_preferred_languages: Some(vec![
                "en".into(), "pt".into(), "es".into(), "de".into(), "nl".into(),
                "en-GB".into(), "fr".into(), "de".into(), "hi".into(), "ja".into(),
            ]),
        }
    }
    fn merge(mut self, v: Value) -> Self {
        if let Some(o) = v.as_object() {
            if let Some(x) = o.get("default_content_processing_engine_doc") { self.default_content_processing_engine_doc = x.as_str().map(str::to_string); }
            if let Some(x) = o.get("default_content_processing_engine_url") { self.default_content_processing_engine_url = x.as_str().map(str::to_string); }
            if let Some(x) = o.get("default_embedding_option") { self.default_embedding_option = x.as_str().map(str::to_string); }
            if let Some(x) = o.get("auto_delete_files") { self.auto_delete_files = x.as_str().map(str::to_string); }
            if let Some(x) = o.get("docling_ocr") { self.docling_ocr = x.as_bool(); }
            if let Some(x) = o.get("docling_formulas") { self.docling_formulas = x.as_bool(); }
            if let Some(x) = o.get("docling_vision") { self.docling_vision = x.as_bool(); }
            if let Some(x) = o.get("youtube_preferred_languages") {
                self.youtube_preferred_languages = x.as_array().map(|a| a.iter().filter_map(|i| i.as_str().map(str::to_string)).collect());
            }
        }
        self
    }
}

async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsResponse>, AppError> {
    let row = sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key = 'content_settings'")
        .fetch_optional(&state.pool)
        .await?;
    let s = match row {
        Some(v) => SettingsResponse::defaults().merge(v),
        None => SettingsResponse::defaults(),
    };
    Ok(Json(s))
}

async fn update_settings(
    State(state): State<AppState>,
    Json(payload): Json<SettingsResponse>,
) -> Result<Json<SettingsResponse>, AppError> {
    let current = get_settings(State(state.clone())).await?.0;
    // ponytail: shallow merge by replacing provided fields; keep defaults for missing stored keys.
    let merged = SettingsResponse {
        default_content_processing_engine_doc: payload.default_content_processing_engine_doc.or(current.default_content_processing_engine_doc),
        default_content_processing_engine_url: payload.default_content_processing_engine_url.or(current.default_content_processing_engine_url),
        default_embedding_option: payload.default_embedding_option.or(current.default_embedding_option),
        auto_delete_files: payload.auto_delete_files.or(current.auto_delete_files),
        docling_ocr: payload.docling_ocr.or(current.docling_ocr),
        docling_formulas: payload.docling_formulas.or(current.docling_formulas),
        docling_vision: payload.docling_vision.or(current.docling_vision),
        youtube_preferred_languages: payload.youtube_preferred_languages.or(current.youtube_preferred_languages),
    };
    let value = json!(merged);
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('content_settings', $1, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
    )
    .bind(&value)
    .execute(&state.pool)
    .await?;
    Ok(Json(merged))
}

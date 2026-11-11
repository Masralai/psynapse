use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/providers", get(list_providers))
}

#[derive(Serialize)]
struct ProviderInfo {
    name: &'static str,
    display_name: &'static str,
    modalities: Vec<&'static str>,
    docs_url: Option<&'static str>,
    env_configured: bool,
}

async fn list_providers(State(state): State<AppState>) -> Json<Vec<ProviderInfo>> {
    // ponytail: static registry subset/order sufficient for UI; env/cred flags best-effort.
    let defs: Vec<(&str, &str, Vec<&str>, Option<&str>, &str)> = vec![
        ("openai","OpenAI",vec!["language","embedding","text_to_speech","speech_to_text"],Some("https://platform.openai.com/api-keys"),"OPENAI_API_KEY"),
        ("anthropic","Anthropic",vec!["language"],Some("https://console.anthropic.com/"),"ANTHROPIC_API_KEY"),
        ("google","Google",vec!["language","embedding","text_to_speech","speech_to_text"],Some("https://aistudio.google.com/apikey"),"GOOGLE_API_KEY"),
        ("groq","Groq",vec!["language","speech_to_text"],Some("https://console.groq.com/keys"),"GROQ_API_KEY"),
        ("mistral","Mistral",vec!["language","embedding"],Some("https://console.mistral.ai/"),"MISTRAL_API_KEY"),
        ("deepseek","DeepSeek",vec!["language"],Some("https://platform.deepseek.com/"),"DEEPSEEK_API_KEY"),
        ("xai","xAI",vec!["language"],Some("https://console.x.ai/"),"XAI_API_KEY"),
        ("openrouter","OpenRouter",vec!["language"],Some("https://openrouter.ai/keys"),"OPENROUTER_API_KEY"),
        ("voyage","Voyage AI",vec!["embedding"],Some("https://dash.voyageai.com/"),"VOYAGE_API_KEY"),
        ("elevenlabs","ElevenLabs",vec!["text_to_speech","speech_to_text"],Some("https://elevenlabs.io/"),"ELEVENLABS_API_KEY"),
        ("azure","Azure OpenAI",vec!["language","embedding","text_to_speech","speech_to_text"],Some("https://portal.azure.com/"),"AZURE_OPENAI_API_KEY"),
        ("ollama","Ollama",vec!["language","embedding"],Some("https://ollama.com/"),"OLLAMA_API_BASE"),
        ("openai_compatible","OpenAI Compatible",vec!["language","embedding","text_to_speech","speech_to_text"],None,"OPENAI_COMPATIBLE_BASE_URL"),
        ("dashscope","DashScope",vec!["language"],Some("https://dashscope.console.aliyun.com/"),"DASHSCOPE_API_KEY"),
        ("minimax","MiniMax",vec!["language"],Some("https://platform.minimaxi.com/"),"MINIMAX_API_KEY"),
        ("vertex","Vertex AI",vec!["language","embedding","text_to_speech","speech_to_text"],Some("https://console.cloud.google.com/"),"VERTEX_PROJECT"),
        ("anthropic_compatible","Anthropic Compatible",vec!["language"],None,"ANTHROPIC_COMPATIBLE_API_KEY"),
    ];
    let mut out = Vec::with_capacity(defs.len());
    for (name, display_name, modalities, docs_url, env_key) in defs {
        let has_cred = sqlx::query_scalar::<_, i64>("SELECT 1 FROM credentials WHERE lower(provider)=lower($1) LIMIT 1")
            .bind(name).fetch_optional(&state.pool).await.ok().flatten().is_some();
        let has_env = std::env::var(env_key).ok().filter(|s| !s.trim().is_empty()).is_some()
            || (name=="google" && std::env::var("GEMINI_API_KEY").ok().filter(|s| !s.trim().is_empty()).is_some());
        out.push(ProviderInfo { name, display_name, modalities, docs_url, env_configured: has_cred || has_env });
    }
    Json(out)
}

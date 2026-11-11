use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub server_port: u16,
    pub encryption_key: String,
    pub upload_dir: String,
    pub job_poll_interval_secs: u64,
    pub job_retry_backoff_secs: u64,
    pub app_password: Option<String>,
    pub max_upload_size_bytes: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let app_password = env::var("OPEN_NOTEBOOK_PASSWORD")
            .ok()
            .or_else(|| env::var("APP_PASSWORD").ok())
            .and_then(|v| {
                let t = v.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            });

        let max_upload_size_mb: f64 = env::var("OPEN_NOTEBOOK_MAX_UPLOAD_SIZE_MB")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|v: &f64| *v > 0.0)
            .unwrap_or(100.0);
        let max_upload_size_bytes = (max_upload_size_mb * 1024.0 * 1024.0) as usize;

        Ok(Config {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost:5432/psynapse".to_string()),
            server_port: env::var("PORT")
                .unwrap_or_else(|_| "5055".to_string())
                .parse()?,
            encryption_key: env::var("ENCRYPTION_KEY")
                .unwrap_or_else(|_| "change-me-to-a-secret-key".to_string()),
            upload_dir: env::var("UPLOAD_DIR")
                .unwrap_or_else(|_| "./data/uploads".to_string()),
            // ponytail: simple fixed polling/backoff; adaptive scheduling and concurrency controls pending.
            job_poll_interval_secs: env::var("JOB_POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            job_retry_backoff_secs: env::var("JOB_RETRY_BACKOFF_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            app_password,
            max_upload_size_bytes,
        })
    }
}

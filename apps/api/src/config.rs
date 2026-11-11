use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub server_port: u16,
    pub encryption_key: String,
    pub upload_dir: String,
    pub job_poll_interval_secs: u64,
    pub job_retry_backoff_secs: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

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
        })
    }
}

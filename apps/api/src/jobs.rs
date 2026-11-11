use crate::errors::AppError;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn enqueue(
    pool: &PgPool,
    app_name: &str,
    command_name: &str,
    input: Value,
) -> Result<Uuid, AppError> {
    let row = sqlx::query(
        r#"
        INSERT INTO jobs (app_name, command_name, input, status)
        VALUES ($1, $2, $3, 'queued')
        RETURNING id
        "#,
    )
    .bind(app_name)
    .bind(command_name)
    .bind(input)
    .fetch_one(pool)
    .await?;
    Ok(row.get("id"))
}

pub async fn status_payload(pool: &PgPool, job_id: Uuid) -> Result<Value, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, status, result, error_message, created_at, started_at, completed_at
        FROM jobs WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let status: String = row.get("status");
    let result: Option<Value> = row.get("result");
    let error_message: Option<String> = row.get("error_message");
    let processing_info = json!({
        "started_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at").map(|t| t.to_rfc3339()),
        "completed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at").map(|t| t.to_rfc3339()),
        "error": error_message,
        "result": result,
    });

    Ok(json!({
        "job_id": row.get::<Uuid, _>("id").to_string(),
        "status": status,
        "result": result,
        "error_message": error_message,
        "created": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        "updated": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at")
            .unwrap_or_else(|| row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"))
            .to_rfc3339(),
        "progress": processing_info,
    }))
}

pub fn as_command_id(job_id: Uuid) -> String {
    format!("command:{}", job_id)
}
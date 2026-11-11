use crate::{errors::AppError, state::AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notebooks", get(list_notebooks).post(create_notebook))
        .route("/recently-viewed", get(recently_viewed))
        .route(
            "/notebooks/:notebook_id",
            get(get_notebook).put(update_notebook).delete(delete_notebook),
        )
        .route(
            "/notebooks/:notebook_id/delete-preview",
            get(delete_preview),
        )
        .route(
            "/notebooks/:notebook_id/sources/:source_id",
            post(add_source).delete(remove_source),
        )
}

#[derive(Serialize)]
pub struct NotebookResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub archived: bool,
    pub created: String,
    pub updated: String,
    pub source_count: i64,
    pub note_count: i64,
}

#[derive(Deserialize)]
pub struct NotebookCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct NotebookUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub archived: Option<bool>,
    pub order_by: Option<String>,
}

#[derive(Deserialize)]
pub struct RecentlyViewedParams {
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct RecentlyViewedResponse {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: String,
    pub title: String,
    pub last_viewed_at: String,
}

#[derive(Serialize)]
pub struct NotebookDeletePreview {
    pub notebook_id: String,
    pub notebook_name: String,
    pub note_count: i64,
    pub exclusive_source_count: i64,
    pub shared_source_count: i64,
}

#[derive(Deserialize)]
pub struct DeleteParams {
    #[serde(default)]
    pub delete_exclusive_sources: bool,
}

#[derive(Serialize)]
pub struct NotebookDeleteResponse {
    pub message: String,
    pub deleted_notes: i64,
    pub deleted_sources: i64,
    pub unlinked_sources: i64,
}

#[derive(Serialize)]
struct OkMsg {
    message: String,
}

fn fmt_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339()
}

fn order_clause(order_by: &str) -> &'static str {
    // ponytail: allowlist covers frontend sort needs; invalid falls back to updated desc.
    match order_by.trim().to_lowercase().as_str() {
        "name asc" => "n.name ASC",
        "name desc" => "n.name DESC",
        "created asc" => "n.created_at ASC",
        "created desc" => "n.created_at DESC",
        "updated asc" => "n.updated_at ASC",
        _ => "n.updated_at DESC",
    }
}

async fn list_notebooks(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<NotebookResponse>>, AppError> {
    let order = order_clause(params.order_by.as_deref().unwrap_or("updated desc"));
    let sql = format!(
        r#"
        SELECT n.id, n.name, n.description, n.archived, n.created_at, n.updated_at,
               (SELECT COUNT(*) FROM source_notebooks sn WHERE sn.notebook_id = n.id) AS source_count,
               (SELECT COUNT(*) FROM note_notebooks nn WHERE nn.notebook_id = n.id) AS note_count
        FROM notebooks n
        WHERE ($1::bool IS NULL OR n.archived = $1)
        ORDER BY {order}
        "#
    );
    let rows = sqlx::query(&sql)
        .bind(params.archived)
        .fetch_all(&state.pool)
        .await?;
    let out = rows
        .into_iter()
        .map(|r| NotebookResponse {
            id: r.get::<Uuid, _>("id").to_string(),
            name: r.get("name"),
            description: r.get("description"),
            archived: r.get("archived"),
            created: fmt_ts(r.get("created_at")),
            updated: fmt_ts(r.get("updated_at")),
            source_count: r.get::<i64, _>("source_count"),
            note_count: r.get::<i64, _>("note_count"),
        })
        .collect();
    Ok(Json(out))
}

async fn create_notebook(
    State(state): State<AppState>,
    Json(payload): Json<NotebookCreate>,
) -> Result<Json<NotebookResponse>, AppError> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Notebook name cannot be empty".into()));
    }
    let row = sqlx::query(
        r#"
        INSERT INTO notebooks (name, description)
        VALUES ($1, $2)
        RETURNING id, name, description, archived, created_at, updated_at
        "#,
    )
    .bind(name)
    .bind(payload.description)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(NotebookResponse {
        id: row.get::<Uuid, _>("id").to_string(),
        name: row.get("name"),
        description: row.get("description"),
        archived: row.get("archived"),
        created: fmt_ts(row.get("created_at")),
        updated: fmt_ts(row.get("updated_at")),
        source_count: 0,
        note_count: 0,
    }))
}

async fn get_notebook(
    State(state): State<AppState>,
    Path(notebook_id): Path<String>,
) -> Result<Json<NotebookResponse>, AppError> {
    let id = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let _ = sqlx::query("UPDATE notebooks SET last_viewed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await;
    let row = sqlx::query(
        r#"
        SELECT n.id, n.name, n.description, n.archived, n.created_at, n.updated_at,
               (SELECT COUNT(*) FROM source_notebooks sn WHERE sn.notebook_id = n.id) AS source_count,
               (SELECT COUNT(*) FROM note_notebooks nn WHERE nn.notebook_id = n.id) AS note_count
        FROM notebooks n WHERE n.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let r = row.ok_or(AppError::NotFound)?;
    Ok(Json(NotebookResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        description: r.get("description"),
        archived: r.get("archived"),
        created: fmt_ts(r.get("created_at")),
        updated: fmt_ts(r.get("updated_at")),
        source_count: r.get::<i64, _>("source_count"),
        note_count: r.get::<i64, _>("note_count"),
    }))
}

async fn update_notebook(
    State(state): State<AppState>,
    Path(notebook_id): Path<String>,
    Json(payload): Json<NotebookUpdate>,
) -> Result<Json<NotebookResponse>, AppError> {
    let id = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let current = sqlx::query("SELECT name, description, archived FROM notebooks WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let name = payload.name.unwrap_or_else(|| current.get("name"));
    if name.trim().is_empty() {
        return Err(AppError::BadRequest("Notebook name cannot be empty".into()));
    }
    let description = payload
        .description
        .unwrap_or_else(|| current.get("description"));
    let archived = payload.archived.unwrap_or_else(|| current.get("archived"));
    let r = sqlx::query(
        r#"
        UPDATE notebooks SET name=$1, description=$2, archived=$3, updated_at=NOW()
        WHERE id=$4
        RETURNING id, name, description, archived, created_at, updated_at
        "#,
    )
    .bind(name.trim())
    .bind(description)
    .bind(archived)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    let counts = sqlx::query(
        r#"
        SELECT
          (SELECT COUNT(*) FROM source_notebooks WHERE notebook_id = $1) AS source_count,
          (SELECT COUNT(*) FROM note_notebooks WHERE notebook_id = $1) AS note_count
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(NotebookResponse {
        id: r.get::<Uuid, _>("id").to_string(),
        name: r.get("name"),
        description: r.get("description"),
        archived: r.get("archived"),
        created: fmt_ts(r.get("created_at")),
        updated: fmt_ts(r.get("updated_at")),
        source_count: counts.get::<i64, _>("source_count"),
        note_count: counts.get::<i64, _>("note_count"),
    }))
}

async fn delete_preview(
    State(state): State<AppState>,
    Path(notebook_id): Path<String>,
) -> Result<Json<NotebookDeletePreview>, AppError> {
    let id = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let nb = sqlx::query("SELECT name FROM notebooks WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let note_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note_notebooks WHERE notebook_id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    let rows = sqlx::query(
        r#"
        SELECT sn.source_id,
               (SELECT COUNT(*) FROM source_notebooks sn2 WHERE sn2.source_id = sn.source_id AND sn2.notebook_id <> $1) AS others
        FROM source_notebooks sn WHERE sn.notebook_id = $1
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let mut exclusive = 0i64;
    let mut shared = 0i64;
    for r in rows {
        if r.get::<i64, _>("others") == 0 {
            exclusive += 1;
        } else {
            shared += 1;
        }
    }
    Ok(Json(NotebookDeletePreview {
        notebook_id: id.to_string(),
        notebook_name: nb.get("name"),
        note_count,
        exclusive_source_count: exclusive,
        shared_source_count: shared,
    }))
}

async fn delete_notebook(
    State(state): State<AppState>,
    Path(notebook_id): Path<String>,
    Query(params): Query<DeleteParams>,
) -> Result<Json<NotebookDeleteResponse>, AppError> {
    let id = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let mut tx = state.pool.begin().await?;
    let exists = sqlx::query("SELECT 1 FROM notebooks WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let note_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM note_notebooks WHERE notebook_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM notes WHERE id IN (SELECT note_id FROM note_notebooks WHERE notebook_id = $1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let src_rows = sqlx::query(
        r#"
        SELECT sn.source_id,
               (SELECT COUNT(*) FROM source_notebooks sn2 WHERE sn2.source_id = sn.source_id AND sn2.notebook_id <> $1) AS others
        FROM source_notebooks sn WHERE sn.notebook_id = $1
        "#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let mut deleted_sources = 0i64;
    let mut unlinked_sources = 0i64;
    if params.delete_exclusive_sources {
        for r in &src_rows {
            let sid: Uuid = r.get("source_id");
            if r.get::<i64, _>("others") == 0 {
                sqlx::query("DELETE FROM sources WHERE id = $1")
                    .bind(sid)
                    .execute(&mut *tx)
                    .await?;
                deleted_sources += 1;
            } else {
                unlinked_sources += 1;
            }
        }
    } else {
        unlinked_sources = src_rows.len() as i64;
    }

    sqlx::query("DELETE FROM notebooks WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(NotebookDeleteResponse {
        message: "Notebook deleted successfully".into(),
        deleted_notes: note_count,
        deleted_sources,
        unlinked_sources,
    }))
}

async fn recently_viewed(
    State(state): State<AppState>,
    Query(params): Query<RecentlyViewedParams>,
) -> Result<Json<Vec<RecentlyViewedResponse>>, AppError> {
    let limit = params.limit.unwrap_or(12).clamp(1, 50);
    let rows = sqlx::query(
        r#"
        SELECT item_type, id, title, last_viewed_at FROM (
            SELECT 'notebook'::text AS item_type, id::text AS id, name AS title, last_viewed_at
            FROM notebooks WHERE last_viewed_at IS NOT NULL
            UNION ALL
            SELECT 'source'::text AS item_type, id::text AS id, COALESCE(title, 'Untitled source') AS title, last_viewed_at
            FROM sources WHERE last_viewed_at IS NOT NULL
        ) t
        ORDER BY last_viewed_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    let out = rows
        .into_iter()
        .map(|r| RecentlyViewedResponse {
            item_type: r.get("item_type"),
            id: r.get("id"),
            title: r.get::<Option<String>, _>("title").unwrap_or_else(|| "Untitled".into()),
            last_viewed_at: fmt_ts(r.get("last_viewed_at")),
        })
        .collect();
    Ok(Json(out))
}

async fn add_source(
    State(state): State<AppState>,
    Path((notebook_id, source_id)): Path<(String, String)>,
) -> Result<Json<OkMsg>, AppError> {
    let nid = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let sid = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    sqlx::query(
        "INSERT INTO source_notebooks (source_id, notebook_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(sid)
    .bind(nid)
    .execute(&state.pool)
    .await?;
    Ok(Json(OkMsg { message: "Source linked to notebook".into() }))
}

async fn remove_source(
    State(state): State<AppState>,
    Path((notebook_id, source_id)): Path<(String, String)>,
) -> Result<Json<OkMsg>, AppError> {
    let nid = Uuid::parse_str(&notebook_id).map_err(|_| AppError::NotFound)?;
    let sid = Uuid::parse_str(&source_id).map_err(|_| AppError::NotFound)?;
    sqlx::query("DELETE FROM source_notebooks WHERE source_id = $1 AND notebook_id = $2")
        .bind(sid)
        .bind(nid)
        .execute(&state.pool)
        .await?;
    Ok(Json(OkMsg { message: "Source unlinked from notebook".into() }))
}

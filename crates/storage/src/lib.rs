use std::path::PathBuf;

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use thiserror::Error;
use vantadeck_domain::{AppInstallation, AppState, DetectionEvidence};

pub struct Storage {
    pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualOverrideRecord {
    pub app_id: String,
    pub version: Version,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredProject {
    pub root: PathBuf,
    pub name: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRecord {
    pub id: i64,
    pub kind: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIndexCache {
    pub source_url: String,
    pub content: String,
    pub etag: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("stored JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stored semantic version is invalid: {0}")]
    Version(#[from] semver::Error),
}

impl Storage {
    pub async fn connect(url: &str) -> Result<Self, StorageError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await?;
        Self::initialize(pool).await
    }

    pub async fn connect_path(path: &std::path::Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        Self::initialize(pool).await
    }

    async fn initialize(pool: SqlitePool) -> Result<Self, StorageError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS preferences (\
                key TEXT PRIMARY KEY NOT NULL,\
                value_json TEXT NOT NULL,\
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS manual_overrides (\
                app_id TEXT NOT NULL, version TEXT NOT NULL, executable_path TEXT NOT NULL,\
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                PRIMARY KEY (app_id, version)\
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS detected_installations (\
                app_id TEXT NOT NULL, executable_path TEXT NOT NULL, version TEXT NOT NULL,\
                state_json TEXT NOT NULL, evidence_json TEXT NOT NULL,\
                last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,\
                PRIMARY KEY (app_id, executable_path)\
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS registered_projects (\
                root_path TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, pinned INTEGER NOT NULL DEFAULT 0,\
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS activity (\
                id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, message TEXT NOT NULL,\
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tool_index_cache (\
                source_url TEXT PRIMARY KEY NOT NULL, content_json TEXT NOT NULL, etag TEXT,\
                fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    pub async fn set_preference(&self, key: &str, value: &Value) -> Result<(), StorageError> {
        let encoded = serde_json::to_string(value)?;
        sqlx::query(
            "INSERT INTO preferences (key, value_json) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, \
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(encoded)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_preference(&self, key: &str) -> Result<Option<Value>, StorageError> {
        let row = sqlx::query("SELECT value_json FROM preferences WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| serde_json::from_str(row.get::<&str, _>("value_json")))
            .transpose()
            .map_err(StorageError::from)
    }

    pub async fn set_manual_override(
        &self,
        record: &ManualOverrideRecord,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO manual_overrides (app_id, version, executable_path) VALUES (?, ?, ?) \
             ON CONFLICT(app_id, version) DO UPDATE SET executable_path = excluded.executable_path, \
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&record.app_id)
        .bind(record.version.to_string())
        .bind(record.executable.to_string_lossy().as_ref())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_manual_override(
        &self,
        app_id: &str,
        version: &Version,
    ) -> Result<Option<ManualOverrideRecord>, StorageError> {
        let row = sqlx::query(
            "SELECT app_id, version, executable_path FROM manual_overrides \
             WHERE app_id = ? AND version = ?",
        )
        .bind(app_id)
        .bind(version.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(ManualOverrideRecord {
                app_id: row.get("app_id"),
                version: Version::parse(row.get::<&str, _>("version"))?,
                executable: PathBuf::from(row.get::<String, _>("executable_path")),
            })
        })
        .transpose()
    }

    pub async fn manual_overrides(
        &self,
        app_id: &str,
    ) -> Result<Vec<ManualOverrideRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT app_id, version, executable_path FROM manual_overrides \
             WHERE app_id = ? ORDER BY version DESC",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ManualOverrideRecord {
                    app_id: row.get("app_id"),
                    version: Version::parse(row.get::<&str, _>("version"))?,
                    executable: PathBuf::from(row.get::<String, _>("executable_path")),
                })
            })
            .collect()
    }

    pub async fn replace_detected_installations(
        &self,
        app_id: &str,
        installations: &[AppInstallation],
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM detected_installations WHERE app_id = ?")
            .bind(app_id)
            .execute(&mut *transaction)
            .await?;
        for installation in installations {
            sqlx::query(
                "INSERT INTO detected_installations \
                 (app_id, executable_path, version, state_json, evidence_json) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(app_id)
            .bind(installation.executable.to_string_lossy().as_ref())
            .bind(installation.version.to_string())
            .bind(serde_json::to_string(&installation.state)?)
            .bind(serde_json::to_string(&installation.evidence)?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn detected_installations(
        &self,
        app_id: &str,
    ) -> Result<Vec<AppInstallation>, StorageError> {
        let rows = sqlx::query(
            "SELECT executable_path, version, state_json, evidence_json \
             FROM detected_installations WHERE app_id = ? ORDER BY version DESC",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AppInstallation {
                    executable: PathBuf::from(row.get::<String, _>("executable_path")),
                    version: Version::parse(row.get::<&str, _>("version"))?,
                    state: serde_json::from_str::<AppState>(row.get("state_json"))?,
                    evidence: serde_json::from_str::<Vec<DetectionEvidence>>(
                        row.get("evidence_json"),
                    )?,
                })
            })
            .collect()
    }

    pub async fn register_project(&self, project: &RegisteredProject) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO registered_projects (root_path, name, pinned) VALUES (?, ?, ?) \
             ON CONFLICT(root_path) DO UPDATE SET name = excluded.name, pinned = excluded.pinned, \
             updated_at = CURRENT_TIMESTAMP",
        )
        .bind(project.root.to_string_lossy().as_ref())
        .bind(&project.name)
        .bind(project.pinned)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn registered_projects(&self) -> Result<Vec<RegisteredProject>, StorageError> {
        let rows = sqlx::query(
            "SELECT root_path, name, pinned FROM registered_projects \
             ORDER BY pinned DESC, name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RegisteredProject {
                root: PathBuf::from(row.get::<String, _>("root_path")),
                name: row.get("name"),
                pinned: row.get("pinned"),
            })
            .collect())
    }

    pub async fn search_projects(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegisteredProject>, StorageError> {
        let pattern = format!("%{}%", query.trim());
        let rows = sqlx::query(
            "SELECT root_path, name, pinned FROM registered_projects \
             WHERE name LIKE ? COLLATE NOCASE OR root_path LIKE ? COLLATE NOCASE \
             ORDER BY pinned DESC, name COLLATE NOCASE LIMIT ?",
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(i64::from(limit.min(500)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RegisteredProject {
                root: PathBuf::from(row.get::<String, _>("root_path")),
                name: row.get("name"),
                pinned: row.get("pinned"),
            })
            .collect())
    }

    pub async fn set_project_pinned(
        &self,
        root: &std::path::Path,
        pinned: bool,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE registered_projects SET pinned = ?, updated_at = CURRENT_TIMESTAMP \
             WHERE root_path = ?",
        )
        .bind(pinned)
        .bind(root.to_string_lossy().as_ref())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_activity(&self, kind: &str, message: &str) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO activity (kind, message) VALUES (?, ?)")
            .bind(kind)
            .bind(message)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn recent_activity(&self, limit: u32) -> Result<Vec<ActivityRecord>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, kind, message, created_at FROM activity ORDER BY id DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ActivityRecord {
                id: row.get("id"),
                kind: row.get("kind"),
                message: row.get("message"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn cache_tool_index(
        &self,
        source_url: &str,
        content: &str,
        etag: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO tool_index_cache (source_url, content_json, etag) VALUES (?, ?, ?) \
             ON CONFLICT(source_url) DO UPDATE SET content_json = excluded.content_json, \
             etag = excluded.etag, fetched_at = CURRENT_TIMESTAMP",
        )
        .bind(source_url)
        .bind(content)
        .bind(etag)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn cached_tool_index(
        &self,
        source_url: &str,
    ) -> Result<Option<ToolIndexCache>, StorageError> {
        let row = sqlx::query(
            "SELECT source_url, content_json, etag, fetched_at FROM tool_index_cache \
             WHERE source_url = ?",
        )
        .bind(source_url)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| ToolIndexCache {
            source_url: row.get("source_url"),
            content: row.get("content_json"),
            etag: row.get("etag"),
            fetched_at: row.get("fetched_at"),
        }))
    }
}

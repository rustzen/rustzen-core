//! Shared Rustzen core primitives.

pub mod api {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ApiResponse<T> {
        pub code: i32,
        pub message: String,
        pub data: T,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub total: Option<i64>,
    }

    impl<T> ApiResponse<T> {
        pub fn new(data: T, total: Option<i64>) -> Self {
            Self { code: 0, message: "Success".to_string(), data, total }
        }

        pub fn success(data: T) -> Self {
            Self::new(data, None)
        }
    }

    impl<T> ApiResponse<Vec<T>> {
        pub fn page(data: Vec<T>, total: i64) -> Self {
            Self::new(data, Some(total))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ErrorEnvelope {
        pub code: i32,
        pub message: String,
        pub data: Option<Value>,
    }

    impl ErrorEnvelope {
        pub fn new(code: i32, message: impl Into<String>) -> Self {
            Self { code, message: message.into(), data: None }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct HttpError {
        pub status: u16,
        pub code: i32,
        pub message: String,
    }

    impl HttpError {
        pub fn new(status: u16, code: i32, message: impl Into<String>) -> Self {
            Self { status, code, message: message.into() }
        }

        pub fn bad_request(code: i32, message: impl Into<String>) -> Self { Self::new(400, code, message) }
        pub fn unauthorized(code: i32, message: impl Into<String>) -> Self { Self::new(401, code, message) }
        pub fn forbidden(code: i32, message: impl Into<String>) -> Self { Self::new(403, code, message) }
        pub fn not_found(code: i32, message: impl Into<String>) -> Self { Self::new(404, code, message) }
        pub fn conflict(code: i32, message: impl Into<String>) -> Self { Self::new(409, code, message) }
        pub fn internal(code: i32, message: impl Into<String>) -> Self { Self::new(500, code, message) }

        pub fn envelope(&self) -> ErrorEnvelope {
            ErrorEnvelope::new(self.code, self.message.clone())
        }
    }
}

pub mod error {
    #[derive(Debug, thiserror::Error)]
    pub enum CoreError {
        #[error("invalid input: {0}")]
        InvalidInput(String),
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
        #[error("sqlite error: {0}")]
        Sqlite(#[from] sqlx::Error),
        #[error("migration error: {0}")]
        Migration(#[from] sqlx::migrate::MigrateError),
        #[error("json error: {0}")]
        Json(#[from] serde_json::Error),
    }
}

pub mod hash {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    pub fn fnv1a64(input: impl AsRef<[u8]>) -> u64 {
        let mut hash = FNV_OFFSET_BASIS;
        for byte in input.as_ref() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }
}

pub mod sqlite {
    use std::{path::{Path, PathBuf}, str::FromStr, time::Duration};

    use sqlx::{
        migrate::Migrator,
        sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
        SqlitePool,
    };

    use crate::error::CoreError;

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum SqliteTuningProfile {
        Minimal,
        Service,
        ReadOptimized,
        Agent,
    }

    impl Default for SqliteTuningProfile {
        fn default() -> Self { Self::Service }
    }

    #[derive(Debug, Clone)]
    pub struct SqlitePoolConfig {
        pub max_connections: u32,
        pub min_connections: u32,
        pub acquire_timeout: Duration,
        pub idle_timeout: Option<Duration>,
        pub busy_timeout: Duration,
        pub tuning_profile: SqliteTuningProfile,
        pub foreign_keys: bool,
    }

    impl Default for SqlitePoolConfig {
        fn default() -> Self {
            Self {
                max_connections: 4,
                min_connections: 1,
                acquire_timeout: Duration::from_secs(10),
                idle_timeout: Some(Duration::from_secs(600)),
                busy_timeout: Duration::from_secs(5),
                tuning_profile: SqliteTuningProfile::Service,
                foreign_keys: true,
            }
        }
    }

    pub fn database_url_from_path(path: impl AsRef<Path>) -> String {
        let path = path.as_ref();
        if let Some(raw) = path.to_str() {
            if raw == ":memory:" || raw.starts_with("sqlite:") {
                return raw.to_string();
            }
        }
        format!("sqlite:///{}", path.display())
    }

    pub async fn connect_sqlite_with_config(
        database_url: &str,
        config: SqlitePoolConfig,
    ) -> Result<SqlitePool, CoreError> {
        ensure_database_directory(database_url)?;
        let connect_options = SqliteConnectOptions::from_str(database_url)?;
        let connect_options = apply_tuning(connect_options, &config);

        Ok(SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .connect_with(connect_options)
            .await?)
    }

    pub async fn connect_sqlite(database_url: &str) -> Result<SqlitePool, CoreError> {
        connect_sqlite_with_config(database_url, SqlitePoolConfig::default()).await
    }

    pub async fn run_migrations(
        pool: &SqlitePool,
        migrator: &'static Migrator,
    ) -> Result<(), CoreError> {
        migrator.run(pool).await?;
        Ok(())
    }

    pub async fn test_connection(pool: &SqlitePool) -> Result<(), CoreError> {
        sqlx::query("SELECT 1").execute(pool).await?;
        Ok(())
    }

    pub async fn run_incremental_vacuum(pool: &SqlitePool) -> Result<(), CoreError> {
        sqlx::query("PRAGMA incremental_vacuum").execute(pool).await?;
        Ok(())
    }

    pub fn ensure_database_directory(database_url: &str) -> Result<(), CoreError> {
        let Some(path) = database_path_from_url(database_url) else { return Ok(()); };
        if path.as_os_str().is_empty() {
            return Err(CoreError::InvalidInput("SQLite database path cannot be empty".to_string()));
        }
        if path.is_dir() {
            return Err(CoreError::InvalidInput("SQLite database path must be a file path, not a directory".to_string()));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }

    fn apply_tuning(options: SqliteConnectOptions, config: &SqlitePoolConfig) -> SqliteConnectOptions {
        let options = options
            .create_if_missing(true)
            .foreign_keys(config.foreign_keys)
            .busy_timeout(config.busy_timeout);

        match config.tuning_profile {
            SqliteTuningProfile::Minimal => options,
            SqliteTuningProfile::Service => options
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal),
            SqliteTuningProfile::ReadOptimized => options
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .pragma("mmap_size", "268435456")
                .pragma("cache_size", "-16000")
                .pragma("temp_store", "MEMORY")
                .pragma("auto_vacuum", "INCREMENTAL"),
            SqliteTuningProfile::Agent => options
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .pragma("mmap_size", "134217728")
                .pragma("cache_size", "-8000")
                .pragma("temp_store", "MEMORY")
                .pragma("auto_vacuum", "INCREMENTAL"),
        }
    }

    fn database_path_from_url(database_url: &str) -> Option<PathBuf> {
        let value = database_url.trim();
        if value == ":memory:" || value == "sqlite::memory:" {
            return None;
        }
        if let Some(path) = value.strip_prefix("sqlite://") {
            return Some(PathBuf::from(path));
        }
        if let Some(path) = value.strip_prefix("sqlite:") {
            if path == ":memory:" || path == "memory:" {
                return None;
            }
            return Some(PathBuf::from(path));
        }
        Some(PathBuf::from(value))
    }
}

pub use api::{ApiResponse, ErrorEnvelope, HttpError};
pub use error::CoreError;
pub use hash::fnv1a64;
pub use sqlite::{
    SqlitePoolConfig, SqliteTuningProfile, connect_sqlite, connect_sqlite_with_config,
    database_url_from_path, ensure_database_directory, run_incremental_vacuum, run_migrations,
    test_connection,
};

#[cfg(test)]
mod tests {
    use super::{ApiResponse, fnv1a64};

    #[test]
    fn fnv1a64_is_stable() {
        assert_eq!(fnv1a64("rustzen"), 0xdb1f_ae62_bd67_d738);
    }

    #[test]
    fn api_page_sets_total() {
        let response = ApiResponse::page(vec![1, 2], 2);
        assert_eq!(response.code, 0);
        assert_eq!(response.total, Some(2));
    }
}

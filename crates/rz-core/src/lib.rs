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
            Self {
                code: 0,
                message: "Success".to_string(),
                data,
                total,
            }
        }

        pub fn with_message(data: T, message: impl Into<String>, total: Option<i64>) -> Self {
            Self {
                code: 0,
                message: message.into(),
                data,
                total,
            }
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
            Self {
                code,
                message: message.into(),
                data: None,
            }
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
            Self {
                status,
                code,
                message: message.into(),
            }
        }

        pub fn bad_request(code: i32, message: impl Into<String>) -> Self {
            Self::new(400, code, message)
        }

        pub fn unauthorized(code: i32, message: impl Into<String>) -> Self {
            Self::new(401, code, message)
        }

        pub fn forbidden(code: i32, message: impl Into<String>) -> Self {
            Self::new(403, code, message)
        }

        pub fn not_found(code: i32, message: impl Into<String>) -> Self {
            Self::new(404, code, message)
        }

        pub fn conflict(code: i32, message: impl Into<String>) -> Self {
            Self::new(409, code, message)
        }

        pub fn internal(code: i32, message: impl Into<String>) -> Self {
            Self::new(500, code, message)
        }

        pub fn envelope(&self) -> ErrorEnvelope {
            ErrorEnvelope::new(self.code, self.message.clone())
        }
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct Pagination {
        pub page: i64,
        pub page_size: i64,
        pub limit: i64,
        pub offset: i64,
    }

    impl Pagination {
        pub fn normalize(
            page: Option<i64>,
            page_size: Option<i64>,
            default_size: i64,
            max_size: i64,
        ) -> Self {
            let max_size = max_size.max(1);
            let page = page.unwrap_or(1).max(1);
            let default_size = default_size.clamp(1, max_size);
            let page_size = page_size.unwrap_or(default_size).clamp(1, max_size);
            let offset = (page - 1) * page_size;
            Self {
                page,
                page_size,
                limit: page_size,
                offset,
            }
        }
    }
}

pub mod error {
    #[derive(Debug, thiserror::Error)]
    pub enum CoreError {
        #[error("invalid input: {0}")]
        InvalidInput(String),
        #[error("not found: {0}")]
        NotFound(String),
        #[error("conflict: {0}")]
        Conflict(String),
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
        #[cfg(feature = "sqlite")]
        #[error("sqlite error: {0}")]
        Sqlite(#[from] sqlx::Error),
        #[cfg(feature = "sqlite")]
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

#[cfg(feature = "logging")]
pub mod logging;
pub mod role_policy;
#[cfg(feature = "sqlite")]
pub mod sqlite_maintenance;
#[cfg(feature = "sqlite")]
pub mod sqlite_query;

#[cfg(feature = "sqlite")]
pub mod sqlite {
    use std::{
        path::{Path, PathBuf},
        str::FromStr,
        time::Duration,
    };

    pub use sqlx::SqlitePool;
    use sqlx::{
        migrate::Migrator,
        sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    };

    use crate::error::CoreError;

    pub const SQLITE_MEMORY: &str = ":memory:";

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum SqliteTuningProfile {
        Minimal,
        Service,
        ReadOptimized,
        Agent,
    }

    impl Default for SqliteTuningProfile {
        fn default() -> Self {
            Self::Service
        }
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

    impl SqlitePoolConfig {
        pub fn minimal() -> Self {
            Self {
                tuning_profile: SqliteTuningProfile::Minimal,
                ..Self::default()
            }
        }

        pub fn service() -> Self {
            Self {
                tuning_profile: SqliteTuningProfile::Service,
                ..Self::default()
            }
        }

        pub fn read_optimized() -> Self {
            Self {
                tuning_profile: SqliteTuningProfile::ReadOptimized,
                ..Self::default()
            }
        }

        pub fn agent() -> Self {
            Self {
                tuning_profile: SqliteTuningProfile::Agent,
                max_connections: 5,
                ..Self::default()
            }
        }

        pub fn with_pool_size(mut self, min_connections: u32, max_connections: u32) -> Self {
            self.min_connections = min_connections;
            self.max_connections = max_connections.max(min_connections);
            self
        }

        pub fn with_acquire_timeout_secs(mut self, seconds: u64) -> Self {
            self.acquire_timeout = Duration::from_secs(seconds);
            self
        }

        pub fn with_idle_timeout_secs(mut self, seconds: Option<u64>) -> Self {
            self.idle_timeout = seconds.map(Duration::from_secs);
            self
        }

        pub fn with_busy_timeout_secs(mut self, seconds: u64) -> Self {
            self.busy_timeout = Duration::from_secs(seconds);
            self
        }

        pub fn with_foreign_keys(mut self, enabled: bool) -> Self {
            self.foreign_keys = enabled;
            self
        }
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
            if raw == SQLITE_MEMORY || raw.starts_with("sqlite:") {
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
        sqlx::query("PRAGMA incremental_vacuum")
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn run_wal_checkpoint_truncate(pool: &SqlitePool) -> Result<(), CoreError> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(pool)
            .await?;
        Ok(())
    }

    pub fn is_row_not_found(error: &sqlx::Error) -> bool {
        matches!(error, sqlx::Error::RowNotFound)
    }

    pub fn ensure_database_directory(database_url: &str) -> Result<(), CoreError> {
        let Some(path) = database_path_from_url(database_url) else {
            return Ok(());
        };
        if path.as_os_str().is_empty() {
            return Err(CoreError::InvalidInput(
                "SQLite database path cannot be empty".to_string(),
            ));
        }
        if path.is_dir() {
            return Err(CoreError::InvalidInput(
                "SQLite database path must be a file path, not a directory".to_string(),
            ));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }

    fn apply_tuning(
        options: SqliteConnectOptions,
        config: &SqlitePoolConfig,
    ) -> SqliteConnectOptions {
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
        if value == SQLITE_MEMORY || value == "sqlite::memory:" {
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

pub use api::{ApiResponse, ErrorEnvelope, HttpError, Pagination};
pub use error::CoreError;
pub use hash::fnv1a64;
#[cfg(feature = "logging")]
pub use logging::{
    DailyLogCleanupConfig, DailyLogCleanupReport, DailyLogWriter, DailyLoggingConfig,
    DailyLoggingGuard, LogFileConfig, LogTarget, LoggingConfig, LoggingError,
    cleanup_expired_daily_logs, cleanup_expired_daily_logs_for_date, daily_log_file_path,
    init_daily_logging, init_logging, is_daily_log_expired, parse_daily_log_date,
};
pub use role_policy::{
    ADMIN_ROLE_CODE, DEFAULT_DEPLOY_CAPABILITY_PREFIX, DEFAULT_DEPLOY_VIEW_CAPABILITY,
    OWNER_ROLE_CODE, RolePolicy, SYSTEM_WILDCARD, VIEW_ACTIONS, VIEWER_ROLE_CODE,
    default_role_allows_capability, default_role_capability_codes,
};
#[cfg(feature = "sqlite")]
pub use sqlite::{
    SQLITE_MEMORY, SqlitePool, SqlitePoolConfig, SqliteTuningProfile, connect_sqlite,
    connect_sqlite_with_config, database_url_from_path, ensure_database_directory,
    is_row_not_found, run_incremental_vacuum, run_migrations, run_wal_checkpoint_truncate,
    test_connection,
};
#[cfg(feature = "sqlite")]
pub use sqlite_maintenance::{
    SqliteMaintenancePlan, SqliteMaintenanceReport, SqlitePragmaSnapshot, WalCheckpointMode,
    WalCheckpointResult, run_sqlite_incremental_vacuum, run_sqlite_maintenance,
    run_sqlite_optimize, run_wal_checkpoint, sqlite_pragma_snapshot,
};
#[cfg(feature = "sqlite")]
pub use sqlite_query::{
    count_with_filters, fetch_with_filters, parse_optional_i16_filter, push_eq, push_ilike,
};

#[cfg(test)]
mod tests {
    use super::{ApiResponse, Pagination, fnv1a64};

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

    #[test]
    fn pagination_normalizes_bounds() {
        let page = Pagination::normalize(Some(0), Some(500), 10, 100);
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 100);
        assert_eq!(page.offset, 0);
    }
}

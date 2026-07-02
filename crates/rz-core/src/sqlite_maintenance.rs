use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum WalCheckpointMode {
    Passive,
    Full,
    Restart,
    Truncate,
}

impl WalCheckpointMode {
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Passive => "PASSIVE",
            Self::Full => "FULL",
            Self::Restart => "RESTART",
            Self::Truncate => "TRUNCATE",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalCheckpointResult {
    pub busy: i64,
    pub log_frames: i64,
    pub checkpointed_frames: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqlitePragmaSnapshot {
    pub journal_mode: String,
    pub synchronous: i64,
    pub page_size: i64,
    pub page_count: i64,
    pub freelist_count: i64,
    pub freelist_bytes: i64,
}

impl SqlitePragmaSnapshot {
    pub fn total_bytes(&self) -> i64 {
        self.page_size.saturating_mul(self.page_count)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteMaintenancePlan {
    pub checkpoint: Option<WalCheckpointMode>,
    pub incremental_vacuum: bool,
    pub optimize: bool,
}

impl SqliteMaintenancePlan {
    pub fn conservative() -> Self {
        Self {
            checkpoint: Some(WalCheckpointMode::Passive),
            incremental_vacuum: false,
            optimize: true,
        }
    }

    pub fn reclaim() -> Self {
        Self {
            checkpoint: Some(WalCheckpointMode::Truncate),
            incremental_vacuum: true,
            optimize: true,
        }
    }

    pub fn with_checkpoint(mut self, mode: Option<WalCheckpointMode>) -> Self {
        self.checkpoint = mode;
        self
    }

    pub fn with_incremental_vacuum(mut self, enabled: bool) -> Self {
        self.incremental_vacuum = enabled;
        self
    }

    pub fn with_optimize(mut self, enabled: bool) -> Self {
        self.optimize = enabled;
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteMaintenanceReport {
    pub before: SqlitePragmaSnapshot,
    pub after: SqlitePragmaSnapshot,
    pub checkpoint: Option<WalCheckpointResult>,
    pub optimized: bool,
    pub vacuumed: bool,
}

pub async fn sqlite_pragma_snapshot(pool: &SqlitePool) -> Result<SqlitePragmaSnapshot, CoreError> {
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await?;
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let freelist_count: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(pool)
        .await?;
    let freelist_bytes = page_size.saturating_mul(freelist_count);

    Ok(SqlitePragmaSnapshot {
        journal_mode,
        synchronous,
        page_size,
        page_count,
        freelist_count,
        freelist_bytes,
    })
}

pub async fn run_wal_checkpoint(
    pool: &SqlitePool,
    mode: WalCheckpointMode,
) -> Result<WalCheckpointResult, CoreError> {
    let sql = format!("PRAGMA wal_checkpoint({})", mode.as_sql());
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) =
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_one(pool)
            .await?;

    Ok(WalCheckpointResult {
        busy,
        log_frames,
        checkpointed_frames,
    })
}

pub async fn run_sqlite_optimize(pool: &SqlitePool) -> Result<(), CoreError> {
    sqlx::query("PRAGMA optimize").execute(pool).await?;
    Ok(())
}

pub async fn run_sqlite_incremental_vacuum(pool: &SqlitePool) -> Result<(), CoreError> {
    sqlx::query("PRAGMA incremental_vacuum")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn run_sqlite_maintenance(
    pool: &SqlitePool,
    plan: SqliteMaintenancePlan,
) -> Result<SqliteMaintenanceReport, CoreError> {
    let before = sqlite_pragma_snapshot(pool).await?;

    let checkpoint = if let Some(mode) = plan.checkpoint {
        Some(run_wal_checkpoint(pool, mode).await?)
    } else {
        None
    };

    if plan.optimize {
        run_sqlite_optimize(pool).await?;
    }

    if plan.incremental_vacuum {
        run_sqlite_incremental_vacuum(pool).await?;
    }

    let after = sqlite_pragma_snapshot(pool).await?;

    Ok(SqliteMaintenanceReport {
        before,
        after,
        checkpoint,
        optimized: plan.optimize,
        vacuumed: plan.incremental_vacuum,
    })
}

#[cfg(test)]
mod tests {
    use super::{SqliteMaintenancePlan, WalCheckpointMode};

    #[test]
    fn checkpoint_mode_maps_to_sql() {
        assert_eq!(WalCheckpointMode::Truncate.as_sql(), "TRUNCATE");
    }

    #[test]
    fn reclaim_plan_is_aggressive() {
        let plan = SqliteMaintenancePlan::reclaim();
        assert_eq!(plan.checkpoint, Some(WalCheckpointMode::Truncate));
        assert!(plan.incremental_vacuum);
        assert!(plan.optimize);
    }
}

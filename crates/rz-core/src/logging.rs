use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{Local, NaiveDate};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt::writer::MakeWriterExt};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LogTarget {
    Stdout,
    File(LogFileConfig),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LogFileConfig {
    pub directory: PathBuf,
    pub file_name: String,
}

impl LogFileConfig {
    pub fn new(directory: impl Into<PathBuf>, file_name: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            file_name: file_name.into(),
        }
    }

    pub fn for_product(product_slug: &str) -> Self {
        Self::new("logs", format!("{product_slug}.log"))
    }

    pub fn path(&self) -> PathBuf {
        self.directory.join(&self.file_name)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LoggingConfig {
    pub env_filter: String,
    pub target: LogTarget,
    pub ansi: bool,
    pub include_target: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DailyLoggingConfig {
    pub directory: PathBuf,
    pub file_prefix: String,
    pub env_filter: String,
    pub stdout: bool,
    pub ansi: bool,
    pub include_target: bool,
}

impl DailyLoggingConfig {
    pub fn new(directory: impl Into<PathBuf>, file_prefix: impl Into<String>) -> Self {
        Self {
            directory: directory.into(),
            file_prefix: file_prefix.into(),
            env_filter: default_filter(),
            stdout: true,
            ansi: false,
            include_target: false,
        }
    }

    pub fn with_env_filter(mut self, env_filter: impl Into<String>) -> Self {
        self.env_filter = env_filter.into();
        self
    }

    pub fn with_stdout(mut self, enabled: bool) -> Self {
        self.stdout = enabled;
        self
    }

    pub fn with_ansi(mut self, ansi: bool) -> Self {
        self.ansi = ansi;
        self
    }

    pub fn with_target(mut self, include_target: bool) -> Self {
        self.include_target = include_target;
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DailyLogCleanupConfig {
    pub directory: PathBuf,
    pub file_prefix: String,
    pub retention_days: u64,
}

impl DailyLogCleanupConfig {
    pub fn new(
        directory: impl Into<PathBuf>,
        file_prefix: impl Into<String>,
        retention_days: u64,
    ) -> Self {
        Self {
            directory: directory.into(),
            file_prefix: file_prefix.into(),
            retention_days,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DailyLogCleanupReport {
    pub directory: PathBuf,
    pub file_prefix: String,
    pub retention_days: u64,
    pub cutoff: NaiveDate,
    pub scanned: u64,
    pub deleted: u64,
}

impl DailyLogCleanupReport {
    pub fn kept(&self) -> u64 {
        self.scanned.saturating_sub(self.deleted)
    }
}

#[derive(Debug)]
pub struct DailyLoggingGuard {
    _file_guard: WorkerGuard,
}

impl LoggingConfig {
    pub fn stdout(env_filter: impl Into<String>) -> Self {
        Self {
            env_filter: env_filter.into(),
            target: LogTarget::Stdout,
            ansi: true,
            include_target: true,
        }
    }

    pub fn file(env_filter: impl Into<String>, file: LogFileConfig) -> Self {
        Self {
            env_filter: env_filter.into(),
            target: LogTarget::File(file),
            ansi: false,
            include_target: true,
        }
    }

    pub fn with_ansi(mut self, ansi: bool) -> Self {
        self.ansi = ansi;
        self
    }

    pub fn with_target(mut self, include_target: bool) -> Self {
        self.include_target = include_target;
        self
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self::stdout(default_filter())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid tracing env filter: {0}")]
    EnvFilter(String),
    #[error("failed to initialize tracing subscriber: {0}")]
    Subscriber(String),
}

pub fn default_filter() -> String {
    "info".to_string()
}

pub fn init_logging(config: LoggingConfig) -> Result<(), LoggingError> {
    let env_filter =
        EnvFilter::try_new(&config.env_filter).unwrap_or_else(|_| EnvFilter::new(default_filter()));

    match config.target {
        LogTarget::Stdout => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(config.ansi)
            .with_target(config.include_target)
            .try_init()
            .map_err(|error| LoggingError::Subscriber(error.to_string())),
        LogTarget::File(file_config) => {
            let file = open_log_file(&file_config)?;
            let shared = Arc::new(Mutex::new(file));
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_ansi(false)
                .with_target(config.include_target)
                .with_writer(move || SharedFileWriter::new(shared.clone()))
                .try_init()
                .map_err(|error| LoggingError::Subscriber(error.to_string()))
        }
    }
}

pub fn init_daily_logging(config: DailyLoggingConfig) -> Result<DailyLoggingGuard, LoggingError> {
    let env_filter = EnvFilter::try_new(&config.env_filter)
        .map_err(|error| LoggingError::EnvFilter(error.to_string()))?;
    let file_appender = DailyLogWriter::new(&config.directory, &config.file_prefix)?;
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    if config.stdout {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(config.ansi)
            .with_target(config.include_target)
            .compact()
            .with_writer(std::io::stdout.and(file_writer))
            .try_init()
            .map_err(|error| LoggingError::Subscriber(error.to_string()))?;
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_ansi(config.ansi)
            .with_target(config.include_target)
            .compact()
            .with_writer(file_writer)
            .try_init()
            .map_err(|error| LoggingError::Subscriber(error.to_string()))?;
    }

    Ok(DailyLoggingGuard {
        _file_guard: file_guard,
    })
}

pub fn open_log_file(config: &LogFileConfig) -> Result<File, io::Error> {
    fs::create_dir_all(&config.directory)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(config.path())
}

pub fn log_file_path(directory: impl AsRef<Path>, file_name: &str) -> PathBuf {
    directory.as_ref().join(file_name)
}

pub fn daily_log_file_path(
    directory: impl AsRef<Path>,
    file_prefix: &str,
    date: NaiveDate,
) -> PathBuf {
    directory
        .as_ref()
        .join(format!("{file_prefix}-{}.log", date.format("%Y-%m-%d")))
}

pub fn cleanup_expired_daily_logs(
    config: &DailyLogCleanupConfig,
) -> Result<DailyLogCleanupReport, LoggingError> {
    cleanup_expired_daily_logs_for_date(config, Local::now().date_naive())
}

pub fn cleanup_expired_daily_logs_for_date(
    config: &DailyLogCleanupConfig,
    today: NaiveDate,
) -> Result<DailyLogCleanupReport, LoggingError> {
    let mut scanned = 0_u64;
    let mut deleted = 0_u64;

    for entry in fs::read_dir(&config.directory)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(file_date) = parse_daily_log_date(&path, &config.file_prefix) else {
            continue;
        };

        scanned += 1;

        if !is_daily_log_expired(file_date, today, config.retention_days) {
            continue;
        }

        fs::remove_file(&path)?;
        deleted += 1;
    }

    Ok(DailyLogCleanupReport {
        directory: config.directory.clone(),
        file_prefix: config.file_prefix.clone(),
        retention_days: config.retention_days,
        cutoff: daily_log_cutoff(today, config.retention_days),
        scanned,
        deleted,
    })
}

pub fn parse_daily_log_date(path: &Path, file_prefix: &str) -> Option<NaiveDate> {
    let file_name = path.file_name()?.to_str()?;
    let prefix = format!("{file_prefix}-");
    let date = file_name.strip_prefix(&prefix)?.strip_suffix(".log")?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

pub fn is_daily_log_expired(file_date: NaiveDate, today: NaiveDate, retention_days: u64) -> bool {
    file_date < daily_log_cutoff(today, retention_days)
}

fn daily_log_cutoff(today: NaiveDate, retention_days: u64) -> NaiveDate {
    today - chrono::Days::new(retention_days)
}

pub struct DailyLogWriter {
    directory: PathBuf,
    file_prefix: String,
    current_date: NaiveDate,
    file: File,
}

impl DailyLogWriter {
    pub fn new(directory: impl AsRef<Path>, file_prefix: &str) -> Result<Self, io::Error> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;
        let current_date = Local::now().date_naive();
        let file = open_daily_log_file(&directory, file_prefix, current_date)?;

        Ok(Self {
            directory,
            file_prefix: file_prefix.to_string(),
            current_date,
            file,
        })
    }

    fn rotate_if_needed(&mut self) -> Result<(), io::Error> {
        let today = Local::now().date_naive();
        if today == self.current_date {
            return Ok(());
        }

        self.file.flush()?;
        self.file = open_daily_log_file(&self.directory, &self.file_prefix, today)?;
        self.current_date = today;
        Ok(())
    }
}

impl Write for DailyLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rotate_if_needed()?;
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn open_daily_log_file(
    directory: &Path,
    file_prefix: &str,
    date: NaiveDate,
) -> Result<File, io::Error> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(daily_log_file_path(directory, file_prefix, date))
}

#[derive(Clone)]
struct SharedFileWriter {
    file: Arc<Mutex<File>>,
}

impl SharedFileWriter {
    fn new(file: Arc<Mutex<File>>) -> Self {
        Self { file }
    }
}

impl Write for SharedFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file
            .lock()
            .map_err(|_| io::Error::other("log file lock poisoned"))?
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .lock()
            .map_err(|_| io::Error::other("log file lock poisoned"))?
            .flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DailyLogCleanupConfig, LogFileConfig, LoggingConfig, cleanup_expired_daily_logs_for_date,
        daily_log_file_path, is_daily_log_expired, parse_daily_log_date,
    };
    use chrono::{Days, NaiveDate};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn file_config_builds_path() {
        let config = LogFileConfig::new("logs", "server.log");
        assert_eq!(config.path(), PathBuf::from("logs/server.log"));
    }

    #[test]
    fn default_stdout_filter_is_info() {
        let config = LoggingConfig::default();
        assert_eq!(config.env_filter, "info");
    }

    #[test]
    fn daily_log_file_path_uses_prefix_and_date() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 3).unwrap();
        let path = daily_log_file_path("logs", "server", date);
        assert_eq!(path, PathBuf::from("logs/server-2026-04-03.log"));
    }

    #[test]
    fn parse_daily_log_date_returns_date_for_matching_file_name() {
        let path = PathBuf::from("logs/server-2026-04-03.log");
        let file_date = parse_daily_log_date(&path, "server").expect("expected log date");

        assert_eq!(file_date, NaiveDate::from_ymd_opt(2026, 4, 3).unwrap());
    }

    #[test]
    fn parse_daily_log_date_ignores_non_matching_file_name() {
        let path = PathBuf::from("logs/other-2026-04-03.log");

        assert!(parse_daily_log_date(&path, "server").is_none());
    }

    #[test]
    fn is_daily_log_expired_matches_retention_window() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let expired_date = today.checked_sub_days(Days::new(10)).unwrap();
        let recent_date = today.checked_sub_days(Days::new(1)).unwrap();

        assert!(is_daily_log_expired(expired_date, today, 7));
        assert!(!is_daily_log_expired(recent_date, today, 7));
        assert!(!is_daily_log_expired(today, today, 7));
    }

    #[test]
    fn cleanup_expired_daily_logs_for_date_deletes_only_expired_matching_files() {
        let directory = temp_log_directory();
        let today = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let expired = daily_log_file_path(&directory, "server", today - Days::new(10));
        let retained = daily_log_file_path(&directory, "server", today - Days::new(1));
        let unrelated = directory.join("other-2026-04-01.log");
        fs::write(&expired, "expired").unwrap();
        fs::write(&retained, "retained").unwrap();
        fs::write(&unrelated, "unrelated").unwrap();

        let config = DailyLogCleanupConfig::new(&directory, "server", 7);
        let report = cleanup_expired_daily_logs_for_date(&config, today).unwrap();

        assert_eq!(report.deleted, 1);
        assert!(!expired.exists());
        assert!(retained.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    fn temp_log_directory() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rz-core-logs-{nanos}"));
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}

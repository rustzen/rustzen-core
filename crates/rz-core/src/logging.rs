use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tracing_subscriber::EnvFilter;

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
    use super::{LogFileConfig, LoggingConfig};
    use std::path::PathBuf;

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
}

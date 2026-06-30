//! Shared Rustzen runtime layout and environment helpers.

use std::{
    env, fmt,
    path::{Component, Path, PathBuf},
};

pub const DEFAULT_FILES_PREFIX: &str = "/resources";
pub const DEFAULT_SQLITE_FILE: &str = "rustzen.db";
pub const DEFAULT_APP_HOST: &str = "0.0.0.0";
pub const DEFAULT_TIMEZONE: &str = "UTC";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeLayout {
    runtime_root: PathBuf,
    files_prefix: String,
}

impl RuntimeLayout {
    pub fn new(runtime_root: impl Into<PathBuf>, files_prefix: impl Into<String>) -> Self {
        Self {
            runtime_root: runtime_root.into(),
            files_prefix: normalize_prefix(files_prefix.into()),
        }
    }

    pub fn for_product(product_slug: &str) -> Self {
        Self::new(format!(".{product_slug}"), DEFAULT_FILES_PREFIX)
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn files_prefix(&self) -> &str {
        &self.files_prefix
    }

    pub fn runtime_root_dir(&self) -> PathBuf {
        self.runtime_root.clone()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.runtime_root_dir().join("data")
    }

    pub fn db_dir(&self) -> PathBuf {
        self.data_dir().join("db")
    }

    pub fn sqlite_path(&self) -> PathBuf {
        self.db_dir().join(DEFAULT_SQLITE_FILE)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.runtime_root_dir().join("logs")
    }

    pub fn web_dir(&self) -> PathBuf {
        self.runtime_root_dir().join("web")
    }

    pub fn web_dist_dir(&self) -> PathBuf {
        self.web_dir().join("dist")
    }

    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir().join("uploads")
    }

    pub fn avatars_dir(&self) -> PathBuf {
        self.data_dir().join("avatars")
    }

    pub fn runtime_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.data_dir(),
            self.db_dir(),
            self.log_dir(),
            self.uploads_dir(),
            self.avatars_dir(),
            self.web_dir(),
        ]
    }

    pub fn avatars_prefix(&self) -> String {
        format!("{}/avatars", self.files_prefix.trim_end_matches('/'))
    }

    pub fn resolve_runtime_path(&self, value: impl AsRef<Path>) -> PathBuf {
        let value = value.as_ref();
        if let Some(raw) = value.to_str() {
            if raw == ":memory:" || raw.starts_with("sqlite:") {
                return PathBuf::from(raw);
            }
        }
        if value.is_absolute() {
            return normalize_path(value);
        }
        normalize_path(&self.absolute_runtime_root().join(value))
    }

    fn absolute_runtime_root(&self) -> PathBuf {
        if self.runtime_root.is_absolute() {
            self.runtime_root.clone()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(&self.runtime_root))
                .unwrap_or_else(|_| self.runtime_root.clone())
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeDefaults {
    pub product_slug: String,
    pub app_host: String,
    pub app_port: u16,
    pub timezone: String,
    pub files_prefix: String,
    pub sqlite_path: Option<String>,
}

impl RuntimeDefaults {
    pub fn new(product_slug: impl Into<String>, app_port: u16) -> Self {
        Self {
            product_slug: product_slug.into(),
            app_host: DEFAULT_APP_HOST.to_string(),
            app_port,
            timezone: DEFAULT_TIMEZONE.to_string(),
            files_prefix: DEFAULT_FILES_PREFIX.to_string(),
            sqlite_path: None,
        }
    }

    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = timezone.into();
        self
    }

    pub fn with_sqlite_path(mut self, sqlite_path: impl Into<String>) -> Self {
        self.sqlite_path = Some(sqlite_path.into());
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub app_host: String,
    pub app_port: u16,
    pub timezone: String,
    pub sqlite_path: PathBuf,
    pub layout: RuntimeLayout,
}

impl RuntimeConfig {
    pub fn from_defaults(defaults: RuntimeDefaults) -> Self {
        let layout =
            RuntimeLayout::new(format!(".{}", defaults.product_slug), defaults.files_prefix);
        let sqlite_path = defaults
            .sqlite_path
            .map(|value| layout.resolve_runtime_path(value))
            .unwrap_or_else(|| layout.sqlite_path());

        Self {
            app_host: defaults.app_host,
            app_port: defaults.app_port,
            timezone: defaults.timezone,
            sqlite_path,
            layout,
        }
    }

    pub fn from_rustzen_env(defaults: RuntimeDefaults) -> Self {
        let runtime_root = EnvReader::string(
            "RUSTZEN_RUNTIME_ROOT",
            format!(".{}", defaults.product_slug),
        );
        let files_prefix = EnvReader::string("RUSTZEN_FILES_PREFIX", defaults.files_prefix);
        let layout = RuntimeLayout::new(runtime_root, files_prefix);

        let sqlite_path = EnvReader::optional_string("RUSTZEN_SQLITE_PATH")
            .or(defaults.sqlite_path)
            .map(|value| layout.resolve_runtime_path(value))
            .unwrap_or_else(|| layout.sqlite_path());

        Self {
            app_host: EnvReader::string("RUSTZEN_APP_HOST", defaults.app_host),
            app_port: EnvReader::u16("RUSTZEN_APP_PORT", defaults.app_port),
            timezone: EnvReader::string("RUSTZEN_TIMEZONE", defaults.timezone),
            sqlite_path,
            layout,
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.app_host, self.app_port)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EnvReader;

impl EnvReader {
    pub fn string(key: &str, default: impl Into<String>) -> String {
        env::var(key).unwrap_or_else(|_| default.into())
    }

    pub fn optional_string(key: &str) -> Option<String> {
        env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    pub fn path_buf(key: &str, default: impl Into<PathBuf>) -> PathBuf {
        env::var(key)
            .map(PathBuf::from)
            .unwrap_or_else(|_| default.into())
    }

    pub fn u16(key: &str, default: u16) -> u16 {
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub fn u32(key: &str, default: u32) -> u32 {
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub fn u64(key: &str, default: u64) -> u64 {
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub fn i64(key: &str, default: i64) -> i64 {
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub fn usize(key: &str, default: usize) -> usize {
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }

    pub fn bool(key: &str, default: bool) -> bool {
        env::var(key)
            .ok()
            .and_then(|value| parse_bool(&value))
            .unwrap_or(default)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnvFileEntry {
    pub key: String,
    pub value: String,
}

impl EnvFileEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnvFileError {
    pub line: usize,
    pub message: String,
}

impl EnvFileError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for EnvFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid env file line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for EnvFileError {}

pub fn parse_env_file(content: &str) -> Result<Vec<EnvFileEntry>, EnvFileError> {
    let mut entries = Vec::new();
    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            return Err(EnvFileError::new(line_number, "missing '='"));
        };
        let key = key.trim();
        if !is_valid_env_key(key) {
            return Err(EnvFileError::new(line_number, "invalid key"));
        }
        entries.push(EnvFileEntry::new(key, unquote_env_value(value.trim())));
    }
    Ok(entries)
}

pub fn render_env_file(entries: impl IntoIterator<Item = EnvFileEntry>) -> String {
    let mut output = String::new();
    for entry in entries {
        output.push_str(&entry.key);
        output.push('=');
        output.push_str(&entry.value);
        output.push('\n');
    }
    output
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn unquote_env_value(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn normalize_prefix(prefix: String) -> String {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return DEFAULT_FILES_PREFIX.to_string();
    }
    if prefix.starts_with('/') {
        prefix.trim_end_matches('/').to_string()
    } else {
        format!("/{}", prefix.trim_end_matches('/'))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_FILES_PREFIX, EnvFileEntry, RuntimeConfig, RuntimeDefaults, RuntimeLayout,
        parse_env_file, render_env_file,
    };
    use std::path::PathBuf;

    #[test]
    fn product_layout_uses_standard_dirs() {
        let layout = RuntimeLayout::for_product("rustzen-admin");
        assert_eq!(layout.runtime_root_dir(), PathBuf::from(".rustzen-admin"));
        assert_eq!(layout.data_dir(), PathBuf::from(".rustzen-admin/data"));
        assert_eq!(layout.db_dir(), PathBuf::from(".rustzen-admin/data/db"));
        assert_eq!(layout.log_dir(), PathBuf::from(".rustzen-admin/logs"));
        assert_eq!(
            layout.web_dist_dir(),
            PathBuf::from(".rustzen-admin/web/dist")
        );
        assert_eq!(layout.files_prefix(), DEFAULT_FILES_PREFIX);
    }

    #[test]
    fn runtime_config_uses_defaults_without_env() {
        let config = RuntimeConfig::from_defaults(RuntimeDefaults::new("rustzen-admin", 9880));
        assert_eq!(config.app_host, "0.0.0.0");
        assert_eq!(config.app_port, 9880);
        assert_eq!(config.bind_addr(), "0.0.0.0:9880");
        assert_eq!(
            config.layout.runtime_root_dir(),
            PathBuf::from(".rustzen-admin")
        );
    }

    #[test]
    fn env_file_parser_handles_export_and_quotes() {
        let entries = parse_env_file(
            r#"
            # comment
            export RUSTZEN_APP_PORT=9880
            RUSTZEN_RUNTIME_ROOT="."
            "#,
        )
        .expect("parse env file");
        assert_eq!(entries[0], EnvFileEntry::new("RUSTZEN_APP_PORT", "9880"));
        assert_eq!(entries[1], EnvFileEntry::new("RUSTZEN_RUNTIME_ROOT", "."));
    }

    #[test]
    fn env_file_renderer_is_stable() {
        let rendered = render_env_file([
            EnvFileEntry::new("RUSTZEN_APP_PORT", "9880"),
            EnvFileEntry::new("RUSTZEN_RUNTIME_ROOT", "."),
        ]);
        assert_eq!(rendered, "RUSTZEN_APP_PORT=9880\nRUSTZEN_RUNTIME_ROOT=.\n");
    }
}

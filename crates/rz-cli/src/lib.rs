//! Shared Rustzen CLI helpers.

use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;

pub const EXIT_FAILURE: i32 = 1;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl OutputFormat {
    pub fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }

    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

impl Default for Verbosity {
    fn default() -> Self {
        Self::Normal
    }
}

impl Verbosity {
    pub fn resolve(
        quiet: bool,
        no_quiet: bool,
        verbose: bool,
        no_verbose: bool,
        default: Self,
    ) -> Self {
        if no_quiet {
            return Self::Normal;
        }
        if quiet {
            return Self::Quiet;
        }
        if no_verbose {
            return Self::Normal;
        }
        if verbose {
            return Self::Verbose;
        }
        default
    }

    pub fn allows_normal(self) -> bool {
        matches!(self, Self::Normal | Self::Verbose)
    }

    pub fn allows_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct CliOutput {
    pub format: OutputFormat,
    pub verbosity: Verbosity,
}

impl CliOutput {
    pub fn new(format: OutputFormat, verbosity: Verbosity) -> Self {
        Self { format, verbosity }
    }

    pub fn from_flags(
        json: bool,
        quiet: bool,
        no_quiet: bool,
        verbose: bool,
        no_verbose: bool,
    ) -> Self {
        Self {
            format: OutputFormat::from_json_flag(json),
            verbosity: Verbosity::resolve(quiet, no_quiet, verbose, no_verbose, Verbosity::Normal),
        }
    }

    pub fn print_text(&self, line: impl Display) {
        if self.verbosity.allows_normal() {
            println!("{line}");
        }
    }

    pub fn print_verbose(&self, line: impl Display) {
        if self.verbosity.allows_verbose() {
            println!("{line}");
        }
    }

    pub fn print_json<T: Serialize>(&self, value: &T) -> Result<(), serde_json::Error> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    }

    pub fn print_result<T>(&self, text: impl Display, value: &T) -> Result<(), serde_json::Error>
    where
        T: Serialize,
    {
        match self.format {
            OutputFormat::Text => {
                self.print_text(text);
                Ok(())
            }
            OutputFormat::Json => self.print_json(value),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ToggleFlag {
    pub enable: bool,
    pub disable: bool,
}

impl ToggleFlag {
    pub fn resolve(self, default: bool) -> bool {
        if self.disable {
            false
        } else if self.enable {
            true
        } else {
            default
        }
    }
}

pub fn resolve_toggle(enable: bool, disable: bool, default: bool) -> bool {
    ToggleFlag { enable, disable }.resolve(default)
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub value: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    #[error("config file does not exist: {0:?}")]
    NotFound(PathBuf),
    #[error("config root must be a JSON object")]
    InvalidRoot,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn discover_config_files(
    explicit: Option<&Path>,
    cwd: &Path,
) -> Result<Vec<PathBuf>, ConfigFileError> {
    if let Some(path) = explicit {
        if !path.exists() {
            return Err(ConfigFileError::NotFound(path.to_path_buf()));
        }
        return Ok(vec![path.to_path_buf()]);
    }

    Ok(vec![
        cwd.join(".rzrc"),
        cwd.join(".rzrc.json"),
        cwd.join("package.json"),
    ])
}

pub fn load_json_config(
    explicit: Option<&Path>,
    package_field: &str,
) -> Result<Option<LoadedConfig>, ConfigFileError> {
    let cwd = std::env::current_dir()?;
    load_json_config_from(explicit, package_field, &cwd)
}

pub fn load_json_config_from(
    explicit: Option<&Path>,
    package_field: &str,
    cwd: &Path,
) -> Result<Option<LoadedConfig>, ConfigFileError> {
    for candidate in discover_config_files(explicit, cwd)? {
        if candidate.exists() {
            if let Some(value) = read_json_config_file(&candidate, package_field)? {
                return Ok(Some(LoadedConfig {
                    path: candidate,
                    value,
                }));
            }
        }
    }
    Ok(None)
}

pub fn read_json_config_file(
    path: &Path,
    package_field: &str,
) -> Result<Option<Value>, ConfigFileError> {
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(Some(Value::Object(Default::default())));
    }

    let value: Value = serde_json::from_str(&content)?;
    if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        return Ok(value.get(package_field).cloned());
    }

    if value.is_object() {
        Ok(Some(value))
    } else {
        Err(ConfigFileError::InvalidRoot)
    }
}

pub fn print_error(error: impl Display) {
    eprintln!("Failed to execute command: {error}");
}

#[cfg(test)]
mod tests {
    use super::{CliOutput, OutputFormat, ToggleFlag, Verbosity, resolve_toggle};

    #[test]
    fn verbosity_rules_are_stable() {
        assert!(!Verbosity::Quiet.allows_normal());
        assert!(Verbosity::Normal.allows_normal());
        assert!(Verbosity::Verbose.allows_verbose());
    }

    #[test]
    fn default_output_is_text() {
        assert_eq!(OutputFormat::default(), OutputFormat::Text);
    }

    #[test]
    fn output_flags_resolve_json_and_verbosity() {
        let output = CliOutput::from_flags(true, false, false, true, false);
        assert_eq!(output.format, OutputFormat::Json);
        assert_eq!(output.verbosity, Verbosity::Verbose);
    }

    #[test]
    fn disable_toggle_wins() {
        let toggle = ToggleFlag {
            enable: true,
            disable: true,
        };
        assert!(!toggle.resolve(true));
        assert!(resolve_toggle(false, false, true));
    }
}

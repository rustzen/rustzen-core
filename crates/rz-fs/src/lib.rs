//! Shared Rustzen filesystem helpers.

use std::{fs, io, path::{Path, PathBuf}};

use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WalkOptions {
    pub follow_links: bool,
    pub include_dirs: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self { follow_links: false, include_dirs: true }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PathStats {
    pub entries: u64,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RemoveOutcome {
    NotFound,
    FileRemoved,
    DirectoryRemoved,
}

pub fn collect_path_stats(path: impl AsRef<Path>) -> io::Result<PathStats> {
    collect_path_stats_with_options(path, WalkOptions::default())
}

pub fn collect_path_stats_with_options(
    path: impl AsRef<Path>,
    options: WalkOptions,
) -> io::Result<PathStats> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;

    if metadata.is_file() {
        return Ok(PathStats { entries: 1, files: 1, dirs: 0, bytes: metadata.len() });
    }

    let mut stats = PathStats::default();
    for entry in WalkDir::new(path).follow_links(options.follow_links) {
        let entry = entry.map_err(io::Error::other)?;
        let metadata = entry.metadata().map_err(io::Error::other)?;

        if metadata.is_dir() {
            if entry.path() == path {
                continue;
            }
            stats.dirs += 1;
            if options.include_dirs {
                stats.entries += 1;
            }
            continue;
        }

        if metadata.is_file() {
            stats.files += 1;
            stats.entries += 1;
            stats.bytes += metadata.len();
        }
    }

    Ok(stats)
}

pub fn remove_path(path: impl AsRef<Path>) -> io::Result<RemoveOutcome> {
    let path = path.as_ref();
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(RemoveOutcome::NotFound);
    };

    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
        Ok(RemoveOutcome::DirectoryRemoved)
    } else {
        fs::remove_file(path)?;
        Ok(RemoveOutcome::FileRemoved)
    }
}

pub fn ensure_dir(path: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(path)
}

pub fn ensure_dirs<I, P>(dirs: I) -> io::Result<()>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    for dir in dirs {
        ensure_dir(dir)?;
    }
    Ok(())
}

pub fn canonicalize_within(
    root: impl AsRef<Path>,
    candidate: impl AsRef<Path>,
) -> io::Result<PathBuf> {
    let root = root.as_ref().canonicalize()?;
    let candidate = candidate.as_ref().canonicalize()?;

    if candidate.starts_with(&root) {
        Ok(candidate)
    } else {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "path is outside the allowed root"))
    }
}

#[cfg(test)]
mod tests {
    use super::{RemoveOutcome, remove_path};

    #[test]
    fn remove_missing_path_is_not_found() {
        let path = std::env::temp_dir().join("rz-fs-missing-path");
        let outcome = remove_path(path).expect("remove missing path");
        assert_eq!(outcome, RemoveOutcome::NotFound);
    }
}

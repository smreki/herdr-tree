use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use crate::config::Config;

use super::template::{MARKERS, render_managed_blocks};

#[derive(Debug, Error)]
pub enum ManagedConfigError {
    #[error("failed to read Herdr config {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("refusing to modify {path}: {reason}")]
    Conflict { path: String, reason: String },
    #[error("failed to write Herdr config {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
}

pub fn configure(path: &Path, config: &Config) -> Result<PathBuf, ManagedConfigError> {
    let original = read(path)?;
    let without = strip_managed(&original)?;
    for table in ["[ui.sidebar.spaces]", "[theme.custom]"] {
        if without.lines().any(|line| line.trim() == table) {
            return Err(ManagedConfigError::Conflict {
                path: path.display().to_string(),
                reason: format!("{table} already exists outside the herdr-tree block"),
            });
        }
    }
    let backup = backup(path, &original)?;
    let next = format!(
        "{}\n\n{}\n",
        without.trim_end(),
        render_managed_blocks(config)
    );
    write_atomic(path, &next)?;
    Ok(backup)
}

pub fn unconfigure(path: &Path) -> Result<Option<PathBuf>, ManagedConfigError> {
    let original = read(path)?;
    let next = strip_managed(&original)?;
    if next == original {
        return Ok(None);
    }
    let backup = backup(path, &original)?;
    write_atomic(path, &format!("{}\n", next.trim_end()))?;
    Ok(Some(backup))
}

fn strip_managed(text: &str) -> Result<String, ManagedConfigError> {
    let mut output = text.to_owned();
    for (start, end) in MARKERS {
        match (output.find(start), output.find(end)) {
            (None, None) => {}
            (Some(begin), Some(finish)) if finish >= begin => {
                let finish = finish + end.len();
                output.replace_range(begin..finish, "");
            }
            _ => {
                return Err(ManagedConfigError::Conflict {
                    path: "config.toml".into(),
                    reason: format!("unbalanced managed marker {start}"),
                });
            }
        }
    }
    Ok(output)
}

fn read(path: &Path) -> Result<String, ManagedConfigError> {
    fs::read_to_string(path).map_err(|source| ManagedConfigError::Read {
        path: path.display().to_string(),
        source,
    })
}

fn backup(path: &Path, contents: &str) -> Result<PathBuf, ManagedConfigError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup = path.with_extension(format!(
        "toml.herdr-tree.{stamp}.{}.bak",
        std::process::id()
    ));
    fs::write(&backup, contents).map_err(|source| ManagedConfigError::Write {
        path: backup.display().to_string(),
        source,
    })?;
    Ok(backup)
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), ManagedConfigError> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut file = File::create(&temporary).map_err(|source| ManagedConfigError::Write {
        path: temporary.display().to_string(),
        source,
    })?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| ManagedConfigError::Write {
            path: temporary.display().to_string(),
            source,
        })?;
    fs::rename(&temporary, path).map_err(|source| ManagedConfigError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_and_unconfigure_preserve_unrelated_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, "theme = \"tokyonight\"\n[ui]\nsidebar = true\n").expect("write fixture");
        configure(&path, &Config::default()).expect("configure");
        let installed = fs::read_to_string(&path).expect("read configured file");
        assert!(installed.contains("[ui.sidebar.spaces]"));
        unconfigure(&path).expect("unconfigure");
        let removed = fs::read_to_string(&path).expect("read unconfigured file");
        assert!(removed.contains("theme = \"tokyonight\""));
        assert!(!removed.contains("herdr-tree managed"));
    }

    #[test]
    fn foreign_sidebar_table_is_refused() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("config.toml");
        fs::write(&path, "[ui.sidebar.spaces]\nrows = []\n").expect("write fixture");
        assert!(configure(&path, &Config::default()).is_err());
    }
}

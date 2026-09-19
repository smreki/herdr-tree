use std::{fs, path::Path};

use thiserror::Error;

use super::Config;

const DEFAULT_CONFIG: &str = include_str!("../../config.example.toml");

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to create config directory {path}: {source}")]
    CreateDirectory {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read config {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to write default config {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid config {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("invalid config {path}: {message}")]
    Validate { path: String, message: String },
}

pub fn load_or_seed(config_dir: &Path) -> Result<Config, ConfigError> {
    fs::create_dir_all(config_dir).map_err(|source| ConfigError::CreateDirectory {
        path: config_dir.display().to_string(),
        source,
    })?;
    let path = config_dir.join("config.toml");
    if !path.exists() {
        fs::write(&path, DEFAULT_CONFIG).map_err(|source| ConfigError::Write {
            path: path.display().to_string(),
            source,
        })?;
    }
    let text = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })?;
    config.validate().map_err(|message| ConfigError::Validate {
        path: path.display().to_string(),
        message,
    })?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_load_seeds_the_example() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let config = load_or_seed(directory.path()).expect("valid seeded config");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(directory.path().join("config.toml"), "surprise = true\n")
            .expect("write fixture");
        assert!(load_or_seed(directory.path()).is_err());
    }
}

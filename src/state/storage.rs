use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use thiserror::Error;

use super::PersistentState;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("state I/O failed for {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("state file {path} is malformed; it was left untouched: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}

#[derive(Clone, Debug)]
pub struct StateStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl StateStore {
    #[must_use]
    pub fn new(state_dir: &Path) -> Self {
        Self {
            path: state_dir.join("state.json"),
            lock_path: state_dir.join("state.lock"),
        }
    }

    pub fn read(&self) -> Result<PersistentState, StateError> {
        let _lock = self.lock()?;
        self.read_unlocked()
    }

    pub fn update<T>(
        &self,
        update: impl FnOnce(&mut PersistentState) -> T,
    ) -> Result<T, StateError> {
        let _lock = self.lock()?;
        let mut state = self.read_unlocked()?;
        let value = update(&mut state);
        self.write_unlocked(&state)?;
        Ok(value)
    }

    fn lock(&self) -> Result<File, StateError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Self::io_error(parent, source))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|source| Self::io_error(&self.lock_path, source))?;
        file.lock_exclusive()
            .map_err(|source| Self::io_error(&self.lock_path, source))?;
        Ok(file)
    }

    fn read_unlocked(&self) -> Result<PersistentState, StateError> {
        if !self.path.exists() {
            return Ok(PersistentState {
                version: 1,
                ..PersistentState::default()
            });
        }
        let mut text = String::new();
        File::open(&self.path)
            .and_then(|mut file| file.read_to_string(&mut text))
            .map_err(|source| Self::io_error(&self.path, source))?;
        serde_json::from_str(&text).map_err(|source| StateError::Json {
            path: self.path.display().to_string(),
            source,
        })
    }

    fn write_unlocked(&self, state: &PersistentState) -> Result<(), StateError> {
        let temporary = self
            .path
            .with_extension(format!("tmp.{}", std::process::id()));
        let bytes = serde_json::to_vec_pretty(state).map_err(|source| StateError::Json {
            path: self.path.display().to_string(),
            source,
        })?;
        let mut file =
            File::create(&temporary).map_err(|source| Self::io_error(&temporary, source))?;
        file.write_all(&bytes)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|source| Self::io_error(&temporary, source))?;
        fs::rename(&temporary, &self.path).map_err(|source| Self::io_error(&self.path, source))
    }

    fn io_error(path: &Path, source: std::io::Error) -> StateError {
        StateError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_under_lock() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = StateStore::new(directory.path());
        store
            .update(|state| {
                state
                    .sessions
                    .entry("socket".into())
                    .or_default()
                    .collapsed_workspaces
                    .insert("w".into());
            })
            .expect("write state");
        let state = store.read().expect("read state");
        assert!(state.sessions["socket"].collapsed_workspaces.contains("w"));
    }

    #[test]
    fn malformed_state_is_not_overwritten() {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(directory.path().join("state.json"), "not-json").expect("write fixture");
        let store = StateStore::new(directory.path());
        assert!(store.update(|_| ()).is_err());
        assert_eq!(
            fs::read_to_string(directory.path().join("state.json")).expect("read fixture"),
            "not-json"
        );
    }
}

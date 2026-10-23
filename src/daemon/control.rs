use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use std::os::unix::process::CommandExt;

use anyhow::{Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    Wake,
    Refresh,
    Stop,
}

impl ControlCommand {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "wake" => Some(Self::Wake),
            "refresh" => Some(Self::Refresh),
            "stop" => Some(Self::Stop),
            _ => None,
        }
    }
}

pub fn notify(path: &Path, command: ControlCommand) -> Result<()> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("daemon is not running at {}", path.display()))?;
    let value = match command {
        ControlCommand::Wake => "wake\n",
        ControlCommand::Refresh => "refresh\n",
        ControlCommand::Stop => "stop\n",
    };
    stream.write_all(value.as_bytes())?;
    Ok(())
}

pub fn bind(path: &Path) -> Result<UnixListener> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            anyhow::bail!("herdr-tree daemon is already running");
        }
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove stale socket {}", path.display()))?;
    }
    let listener = UnixListener::bind(path)
        .with_context(|| format!("failed to bind control socket {}", path.display()))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub fn poll(listener: &UnixListener) -> Result<Option<ControlCommand>> {
    match listener.accept() {
        Ok((mut stream, _)) => {
            let mut bytes = [0_u8; 32];
            let read = stream.read(&mut bytes)?;
            Ok(std::str::from_utf8(&bytes[..read])
                .ok()
                .and_then(ControlCommand::parse))
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[must_use]
pub fn socket_path(state_dir: &Path, herdr_socket: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    herdr_socket.hash(&mut hasher);
    state_dir.join(format!("daemon-{:016x}.sock", hasher.finish()))
}

pub fn start_background(state_dir: &Path, herdr_socket: &Path) -> Result<()> {
    let socket = socket_path(state_dir, herdr_socket);
    if notify(&socket, ControlCommand::Wake).is_ok() {
        return Ok(());
    }
    std::fs::create_dir_all(state_dir)?;
    let executable = std::env::current_exe().context("failed to resolve herdr-tree executable")?;
    Command::new(executable)
        .args(["daemon", "run"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch herdr-tree daemon")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_controls_are_isolated_in_a_shared_plugin_state_directory() {
        let directory = tempfile::tempdir().expect("temporary plugin state directory");
        let default_path = socket_path(directory.path(), Path::new("/herdr/herdr.sock"));
        let named_path = socket_path(
            directory.path(),
            Path::new("/herdr/sessions/new/herdr.sock"),
        );
        let default = bind(&default_path).expect("bind default session control");
        let named = bind(&named_path).expect("bind named session control");

        notify(&named_path, ControlCommand::Stop).expect("stop named session");
        assert_eq!(poll(&default).expect("poll default session"), None);
        assert_eq!(
            poll(&named).expect("poll named session"),
            Some(ControlCommand::Stop)
        );

        notify(&default_path, ControlCommand::Refresh).expect("refresh default session");
        assert_eq!(poll(&named).expect("poll named session"), None);
        assert_eq!(
            poll(&default).expect("poll default session"),
            Some(ControlCommand::Refresh)
        );
    }

    #[test]
    fn starting_a_session_does_not_mistake_another_sessions_daemon_for_its_own() {
        let directory = tempfile::tempdir().expect("temporary plugin state directory");
        let herdr_socket = Path::new("/herdr/sessions/new/herdr.sock");
        let path = socket_path(directory.path(), herdr_socket);
        let _listener = bind(&path).expect("bind session control");

        assert!(
            notify(
                &socket_path(directory.path(), Path::new("/herdr/herdr.sock")),
                ControlCommand::Wake
            )
            .is_err()
        );
        assert_eq!(path, socket_path(directory.path(), herdr_socket));
    }
}

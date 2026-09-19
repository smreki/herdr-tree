use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
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

pub fn start_background(state_dir: &Path) -> Result<()> {
    let socket = state_dir.join("daemon.sock");
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

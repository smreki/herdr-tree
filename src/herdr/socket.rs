use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("failed to connect to Herdr socket {path}: {source}")]
    Connect {
        path: String,
        source: std::io::Error,
    },
    #[error("Herdr socket I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Herdr returned invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Herdr returned an error: {0}")]
    Api(String),
    #[error("Herdr closed the socket without a response")]
    EmptyResponse,
}

pub fn request(socket_path: &Path, request: &Value) -> Result<Value, SocketError> {
    let mut stream = UnixStream::connect(socket_path).map_err(|source| SocketError::Connect {
        path: socket_path.display().to_string(),
        source,
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    serde_json::to_writer(&mut stream, request)?;
    stream.write_all(b"\n")?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response)?;
    if response.is_empty() {
        return Err(SocketError::EmptyResponse);
    }
    let envelope: Value = serde_json::from_str(&response)?;
    if let Some(error) = envelope.get("error") {
        return Err(SocketError::Api(error.to_string()));
    }
    Ok(envelope)
}

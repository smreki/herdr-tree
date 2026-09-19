use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

use super::{SessionSnapshot, socket};

#[derive(Clone, Debug)]
pub struct HerdrClient {
    socket_path: PathBuf,
}

#[derive(Deserialize)]
struct Envelope<T> {
    result: T,
}

#[derive(Deserialize)]
struct SnapshotResult {
    snapshot: SessionSnapshot,
}

impl HerdrClient {
    #[must_use]
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    #[must_use]
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot> {
        let value = socket::request(
            &self.socket_path,
            &json!({"id": "herdr-tree-snapshot", "method": "session.snapshot", "params": {}}),
        )?;
        let envelope: Envelope<SnapshotResult> = serde_json::from_value(value)
            .context("Herdr session.snapshot response had an unexpected shape")?;
        Ok(envelope.result.snapshot)
    }

    pub fn report_metadata(
        &self,
        workspace_id: &str,
        seq: u64,
        tokens: &BTreeMap<String, Option<String>>,
    ) -> Result<()> {
        if tokens.len() > 16 {
            bail!("metadata patch contains more than Herdr's 16-token limit");
        }
        socket::request(
            &self.socket_path,
            &json!({
                "id": format!("herdr-tree-{seq}"),
                "method": "workspace.report_metadata",
                "params": {
                    "workspace_id": workspace_id,
                    "source": "herdr-tree",
                    "seq": seq,
                    "tokens": tokens,
                }
            }),
        )?;
        Ok(())
    }
}

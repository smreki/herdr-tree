use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde_json::json;

use crate::{
    config::Config,
    git::GitCache,
    herdr::{HerdrClient, chunks},
    project::{LifecycleTracker, TreeView, project_snapshot},
    render::{RenderClock, WorkspaceTokens, diff_tokens, render_workspace},
    state::StateStore,
};

use super::{ControlCommand, control};

const WAKE_DEBOUNCE: Duration = Duration::from_millis(10);
const SERVICE_POLL_INTERVAL: Duration = Duration::from_millis(10);
// Events are wake hints, not a guarantee. Herdr only reports agent status
// changes to per-pane subscribers, so a short snapshot poll keeps lifecycle
// state converging even when no structural event arrives.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

/// Session-wide event subscriptions. Herdr rejects the whole
/// `events.subscribe` request when any entry is malformed, and the pane-scoped
/// kinds (`pane.agent_status_changed`, `pane.output_matched`,
/// `pane.scroll_changed`) require a concrete `pane_id`, so they cannot be
/// requested for the session as a whole.
const SESSION_SUBSCRIPTIONS: [&str; 20] = [
    "workspace.created",
    "workspace.updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.closed",
    "workspace.focused",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.closed",
    "pane.updated",
    "pane.focused",
    "pane.moved",
    "pane.exited",
    "pane.agent_detected",
    "layout.updated",
];

pub struct Service {
    client: HerdrClient,
    config: Config,
    collapsed: BTreeSet<String>,
    workspace_directories: BTreeMap<String, String>,
    workspace_names: BTreeMap<String, String>,
    tab_names: BTreeMap<String, String>,
    rendered: HashMap<String, WorkspaceTokens>,
    tree: Option<TreeView>,
    git: GitCache,
    lifecycle: LifecycleTracker,
    sequence: u64,
    frame: usize,
    state_store: StateStore,
    session_key: String,
    persisted: Option<OwnedState>,
}

/// The session fields this daemon owns, kept to skip unchanged state writes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedState {
    workspace_directories: BTreeMap<String, String>,
    workspace_names: BTreeMap<String, String>,
    tab_names: BTreeMap<String, String>,
    held_done_tabs: BTreeSet<String>,
    held_blocked_tabs: BTreeSet<String>,
}

pub struct ServiceState {
    pub collapsed: BTreeSet<String>,
    pub workspace_directories: BTreeMap<String, String>,
    pub workspace_names: BTreeMap<String, String>,
    pub tab_names: BTreeMap<String, String>,
    pub lifecycle: LifecycleTracker,
}

#[derive(Debug, PartialEq, Eq)]
enum EventHint {
    Wake,
    WorkspaceRenamed { id: String, label: String },
    TabRenamed { id: String, label: String },
}

impl Service {
    #[must_use]
    pub fn new(
        client: HerdrClient,
        config: Config,
        state: ServiceState,
        state_store: StateStore,
        session_key: String,
    ) -> Self {
        Self {
            client,
            config,
            collapsed: state.collapsed,
            workspace_directories: state.workspace_directories,
            workspace_names: state.workspace_names,
            tab_names: state.tab_names,
            rendered: HashMap::new(),
            tree: None,
            git: GitCache::default(),
            lifecycle: state.lifecycle,
            sequence: sequence_seed(),
            frame: 0,
            state_store,
            session_key,
            persisted: None,
        }
    }

    pub fn run(mut self, state_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(state_dir)?;
        let control_path = state_dir.join("daemon.sock");
        let listener = control::bind(&control_path)?;
        let (event_tx, event_rx) = mpsc::channel();
        let event_socket = self.client.socket_path().to_path_buf();
        thread::spawn(move || run_event_subscriber(&event_socket, &event_tx));

        self.reconcile(true)?;
        let animation = Duration::from_millis(self.config.spinner_interval_ms);
        let mut next_animation = Instant::now() + animation;
        let mut next_snapshot = Instant::now() + SNAPSHOT_INTERVAL;
        let git_tick = Duration::from_secs(
            self.config
                .git_refresh_interval_seconds
                .min(self.config.git_background_refresh_interval_seconds),
        );
        let mut next_git_refresh = Instant::now() + git_tick;
        let mut wake_deadline = None;
        let mut force_git_on_wake = false;
        loop {
            if let Some(command) = control::poll(&listener)? {
                match command {
                    ControlCommand::Stop => break,
                    ControlCommand::Wake => {
                        wake_deadline = Some(Instant::now() + WAKE_DEBOUNCE);
                        force_git_on_wake = true;
                    }
                    ControlCommand::Refresh => {
                        self.reconcile(true)?;
                        next_snapshot = Instant::now() + SNAPSHOT_INTERVAL;
                    }
                }
            }
            while let Ok(event) = event_rx.try_recv() {
                self.apply_event(event)?;
                wake_deadline = Some(Instant::now() + WAKE_DEBOUNCE);
            }
            let now = Instant::now();
            let git_due = now >= next_git_refresh;
            if wake_deadline.is_some_and(|deadline| now >= deadline)
                || now >= next_snapshot
                || git_due
            {
                if let Err(error) = self.reconcile(force_git_on_wake) {
                    eprintln!("herdr-tree: reconciliation failed: {error:#}");
                }
                wake_deadline = None;
                force_git_on_wake = false;
                next_snapshot = now + SNAPSHOT_INTERVAL;
                if git_due {
                    next_git_refresh = now + git_tick;
                }
            }
            if now >= next_animation {
                self.frame = self.frame.wrapping_add(1);
                if let Err(error) = self.publish_current() {
                    eprintln!("herdr-tree: animation update failed: {error:#}");
                }
                next_animation = now + animation;
            }
            thread::sleep(SERVICE_POLL_INTERVAL);
        }
        let _ = std::fs::remove_file(&control_path);
        Ok(())
    }

    pub fn reconcile(&mut self, force_git: bool) -> Result<()> {
        let snapshot = self.client.snapshot()?;
        capture_workspace_directories(&snapshot, &mut self.workspace_directories);
        let live_workspaces: BTreeSet<_> = snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        let live_tabs: BTreeSet<_> = snapshot
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect();
        self.workspace_names
            .retain(|id, _| live_workspaces.contains(id.as_str()));
        self.workspace_directories
            .retain(|id, _| live_workspaces.contains(id.as_str()));
        self.tab_names
            .retain(|id, _| live_tabs.contains(id.as_str()));
        let mut tree = project_snapshot(
            &snapshot,
            &self.collapsed,
            &self.workspace_directories,
            &self.workspace_names,
            &self.tab_names,
        );
        self.lifecycle
            .apply(&mut tree, &self.config, Instant::now());
        let (held_done_tabs, held_blocked_tabs) = self.lifecycle.held();
        self.persist(OwnedState {
            workspace_directories: self.workspace_directories.clone(),
            workspace_names: self.workspace_names.clone(),
            tab_names: self.tab_names.clone(),
            held_done_tabs,
            held_blocked_tabs,
        })?;
        for workspace in &mut tree.workspaces {
            for tab in &mut workspace.tabs {
                let Some(directory) = tab.directory.as_deref() else {
                    continue;
                };
                let seconds =
                    if force_git || tab.focused || tab.status == crate::project::TabStatus::Working
                    {
                        self.config.git_refresh_interval_seconds
                    } else {
                        self.config.git_background_refresh_interval_seconds
                    };
                match self.git.status(
                    directory,
                    if force_git {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(seconds)
                    },
                    Duration::from_secs(self.config.git_command_timeout_seconds),
                ) {
                    Ok(status) => tab.git = status,
                    Err(error) => eprintln!(
                        "herdr-tree: Git refresh failed for {}: {error:#}",
                        directory.display()
                    ),
                }
            }
        }
        self.tree = Some(tree);
        self.publish_current()
    }

    fn apply_event(&mut self, event: EventHint) -> Result<()> {
        match event {
            EventHint::Wake => return Ok(()),
            EventHint::WorkspaceRenamed { id, label } => {
                self.workspace_names.insert(id, label);
            }
            EventHint::TabRenamed { id, label } => {
                self.tab_names.insert(id, label);
            }
        }
        let (held_done_tabs, held_blocked_tabs) = self.lifecycle.held();
        self.persist(OwnedState {
            workspace_directories: self.workspace_directories.clone(),
            workspace_names: self.workspace_names.clone(),
            tab_names: self.tab_names.clone(),
            held_done_tabs,
            held_blocked_tabs,
        })
    }

    /// Writes the owned session fields, skipping the atomic replacement when
    /// nothing this daemon owns has changed since the last successful write.
    fn persist(&mut self, owned: OwnedState) -> Result<()> {
        if self.persisted.as_ref() == Some(&owned) {
            return Ok(());
        }
        self.state_store.update(|state| {
            let session = state.sessions.entry(self.session_key.clone()).or_default();
            session
                .workspace_directories
                .clone_from(&owned.workspace_directories);
            session
                .manual_workspace_names
                .clone_from(&owned.workspace_names);
            session.manual_tab_names.clone_from(&owned.tab_names);
            session.held_done_tabs.clone_from(&owned.held_done_tabs);
            session
                .held_blocked_tabs
                .clone_from(&owned.held_blocked_tabs);
        })?;
        self.persisted = Some(owned);
        Ok(())
    }

    fn publish_current(&mut self) -> Result<()> {
        let Some(tree) = self.tree.as_ref() else {
            return Ok(());
        };
        let clock = RenderClock {
            spinner_frame: self.frame,
            blocked_quiet: (self.frame / 5) % 2 == 1,
        };
        for workspace in &tree.workspaces {
            let next = render_workspace(workspace, &self.config, clock);
            let delta = self
                .rendered
                .get(&workspace.workspace_id)
                .map_or_else(|| next.clone(), |previous| diff_tokens(previous, &next));
            for chunk in chunks(&delta) {
                self.sequence = self.sequence.wrapping_add(1);
                self.client
                    .report_metadata(&workspace.workspace_id, self.sequence, &chunk)?;
            }
            self.rendered.insert(workspace.workspace_id.clone(), next);
        }
        Ok(())
    }

    #[must_use]
    pub fn diagnostic_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(tree) = &self.tree {
            for workspace in &tree.workspaces {
                lines.push(format!(
                    "workspace {} {:?}: {} tabs{}",
                    workspace.workspace_id,
                    workspace.label,
                    workspace.total_tabs,
                    if workspace.collapsed {
                        " (collapsed)"
                    } else {
                        ""
                    }
                ));
                for tab in &workspace.tabs {
                    lines.push(format!(
                        "  tab {} {} {:?}: status={:?} panes={} agents={} cwd={}",
                        tab.index,
                        tab.tab_id,
                        tab.label,
                        tab.status,
                        tab.pane_count,
                        tab.agents.join("+"),
                        tab.directory
                            .as_ref()
                            .map_or_else(|| "-".into(), |path| path.display().to_string())
                    ));
                }
            }
        }
        lines
    }
}

fn capture_workspace_directories(
    snapshot: &crate::herdr::SessionSnapshot,
    directories: &mut BTreeMap<String, String>,
) {
    for workspace in &snapshot.workspaces {
        if directories.contains_key(&workspace.workspace_id) {
            continue;
        }
        let directory = snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id)
            .find_map(|tab| snapshot.representative_pane(&tab.tab_id))
            .and_then(|pane| pane.foreground_cwd.as_ref().or(pane.cwd.as_ref()));
        if let Some(directory) = directory {
            directories.insert(workspace.workspace_id.clone(), directory.clone());
        }
    }
}

fn sequence_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn run_event_subscriber(socket_path: &Path, events: &Sender<EventHint>) {
    let mut backoff = 1;
    loop {
        match subscribe_once(socket_path, &SESSION_SUBSCRIPTIONS, events) {
            Ok(()) => backoff = 1,
            Err(error) => {
                eprintln!("herdr-tree: event subscription disconnected: {error:#}");
            }
        }
        thread::sleep(Duration::from_secs(backoff));
        backoff = (backoff * 2).min(30);
    }
}

fn subscribe_once(
    socket_path: &Path,
    subscriptions: &[&str],
    events: &Sender<EventHint>,
) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect to {}", socket_path.display()))?;
    let request = json!({
        "id": "herdr-tree-events",
        "method": "events.subscribe",
        "params": {"subscriptions": subscriptions.iter().map(|kind| json!({"type": kind})).collect::<Vec<_>>()}
    });
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    let mut lines = BufReader::new(stream).lines();
    let ack = lines
        .next()
        .transpose()?
        .context("Herdr closed event subscription")?;
    let acknowledged: serde_json::Value = serde_json::from_str(&ack)?;
    if let Some(error) = acknowledged.get("error") {
        anyhow::bail!("Herdr rejected the event subscription: {error}");
    }
    for line in lines {
        let value: serde_json::Value = serde_json::from_str(&line?)?;
        if events.send(parse_event(&value)).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

fn parse_event(value: &serde_json::Value) -> EventHint {
    // Herdr names subscriptions `workspace.renamed` but delivers events as
    // `workspace_renamed`, carrying the same kind in `data.type`.
    let data = value.get("data").unwrap_or(value);
    let event = value
        .get("event")
        .or_else(|| data.get("type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .replace('.', "_");
    match event.as_str() {
        "workspace_renamed" => field(data, "workspace_id")
            .zip(field(data, "label"))
            .map_or(EventHint::Wake, |(id, label)| EventHint::WorkspaceRenamed {
                id,
                label,
            }),
        "tab_renamed" => field(data, "tab_id").zip(field(data, "label")).map_or(
            EventHint::Wake,
            |(id, label)| EventHint::TabRenamed { id, label },
        ),
        _ => EventHint::Wake,
    }
}

fn field(data: &serde_json::Value, name: &str) -> Option<String> {
    data.get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_events_preserve_manual_labels() {
        let event = serde_json::json!({
            "event": "tab_renamed",
            "data": {"type": "tab_renamed", "tab_id": "t1", "label": "api"}
        });
        assert_eq!(
            parse_event(&event),
            EventHint::TabRenamed {
                id: "t1".into(),
                label: "api".into()
            }
        );
    }

    #[test]
    fn renames_are_recognised_from_the_event_payload_kind() {
        let event = serde_json::json!({
            "data": {"type": "workspace_renamed", "workspace_id": "w2", "label": "billing"}
        });
        assert_eq!(
            parse_event(&event),
            EventHint::WorkspaceRenamed {
                id: "w2".into(),
                label: "billing".into()
            }
        );
    }

    #[test]
    fn structural_events_remain_wake_hints() {
        let event = serde_json::json!({
            "event": "workspace_created",
            "data": {"type": "workspace_created", "workspace": {"workspace_id": "w2"}}
        });
        assert_eq!(parse_event(&event), EventHint::Wake);
    }

    #[test]
    fn session_subscriptions_exclude_pane_scoped_kinds() {
        // Herdr fails the whole subscription when an entry needs a `pane_id`,
        // which would leave the daemon without any lifecycle wake hints.
        for kind in [
            "pane.agent_status_changed",
            "pane.output_matched",
            "pane.scroll_changed",
        ] {
            assert!(
                !SESSION_SUBSCRIPTIONS.contains(&kind),
                "{kind} needs a pane"
            );
        }
        assert!(SESSION_SUBSCRIPTIONS.contains(&"workspace.created"));
    }

    #[test]
    fn workspace_directory_is_captured_only_once() {
        let snapshot = snapshot_with_cwd("/projects/first");
        let mut directories = BTreeMap::new();
        capture_workspace_directories(&snapshot, &mut directories);
        capture_workspace_directories(&snapshot_with_cwd("/projects/second"), &mut directories);
        assert_eq!(directories["w"], "/projects/first");
    }

    fn snapshot_with_cwd(cwd: &str) -> crate::herdr::SessionSnapshot {
        serde_json::from_value(serde_json::json!({
            "version": "0.9.1",
            "protocol": 22,
            "workspaces": [{
                "workspace_id": "w",
                "number": 1,
                "label": "workspace",
                "focused": true,
                "tab_count": 1,
                "active_tab_id": "t"
            }],
            "tabs": [{
                "tab_id": "t",
                "workspace_id": "w",
                "number": 1,
                "label": "tab",
                "focused": true,
                "pane_count": 1
            }],
            "panes": [{
                "pane_id": "p",
                "workspace_id": "w",
                "tab_id": "t",
                "focused": true,
                "cwd": cwd
            }],
            "layouts": [],
            "agents": []
        }))
        .expect("valid snapshot fixture")
    }
}

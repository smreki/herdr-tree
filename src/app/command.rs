use std::{collections::BTreeSet, process::Command};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::{
    config,
    daemon::{
        ControlCommand, Service, ServiceState, control_socket_path, notify, start_background,
    },
    herdr::{HerdrClient, chunks},
    project::{LifecycleTracker, project_snapshot},
    render::{RenderClock, empty_workspace_tokens, render_workspace},
    sidebar_config,
    state::StateStore,
};

use super::runtime::{environment_path, herdr_config_path, seed_shell_runtime_paths, socket_path};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Manage the resident renderer.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Notify the daemon after a shell or lifecycle event.
    Wake,
    /// Force a fresh snapshot and Git reconciliation.
    Refresh,
    /// Collapse or expand the focused workspace.
    ToggleCollapse,
    /// Expand all workspaces in this Herdr session.
    ExpandAll,
    /// Collapse all workspaces in this Herdr session.
    CollapseAll,
    /// Install or refresh the managed Herdr sidebar configuration.
    Configure,
    /// Remove managed sidebar configuration and stop the daemon.
    Unconfigure,
    /// Print a safe snapshot and projection diagnostic.
    Doctor,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Start the daemon in the background if needed.
    Start,
    /// Run the service in the foreground.
    Run,
    /// Stop a running service.
    Stop,
}

pub fn run(cli: Cli) -> Result<()> {
    let Cli { command } = cli;
    let config_dir = environment_path("HERDR_PLUGIN_CONFIG_DIR")?;
    let state_dir = environment_path("HERDR_PLUGIN_STATE_DIR")?;
    let config = config::load_or_seed(&config_dir)?;
    seed_shell_runtime_paths(&config_dir, &state_dir)?;
    let socket = socket_path()?;
    let control = control_socket_path(&state_dir, &socket);
    let session_key = socket.display().to_string();
    let store = StateStore::new(&state_dir);

    match command {
        CommandKind::Daemon {
            command: DaemonCommand::Start,
        } => start_background(&state_dir, &socket),
        CommandKind::Daemon {
            command: DaemonCommand::Stop,
        } => notify(&control, ControlCommand::Stop),
        CommandKind::Daemon {
            command: DaemonCommand::Run,
        } => {
            let state = store.read()?;
            let session = state
                .sessions
                .get(&session_key)
                .cloned()
                .unwrap_or_default();
            let collapsed = session.collapsed_workspaces.iter().cloned().collect();
            let lifecycle = LifecycleTracker::from_held(
                &session.held_done_tabs,
                &session.held_blocked_tabs,
                std::time::Instant::now(),
            );
            Service::new(
                HerdrClient::new(socket),
                config,
                ServiceState {
                    collapsed,
                    workspace_directories: session.workspace_directories,
                    workspace_names: session.manual_workspace_names,
                    tab_names: session.manual_tab_names,
                    lifecycle,
                },
                store,
                session_key,
            )
            .run(&state_dir)
        }
        CommandKind::Wake => {
            // Shell hooks intentionally do not start work synchronously.
            let _ = notify(&control, ControlCommand::Wake);
            Ok(())
        }
        CommandKind::Refresh => {
            if notify(&control, ControlCommand::Refresh).is_err() {
                start_background(&state_dir, &socket)?;
            }
            Ok(())
        }
        CommandKind::ToggleCollapse => {
            let client = HerdrClient::new(socket.clone());
            let snapshot = client.snapshot()?;
            let workspace_id = std::env::var("HERDR_WORKSPACE_ID")
                .ok()
                .or(snapshot.focused_workspace_id)
                .context("there is no focused workspace")?;
            store.update(|state| {
                let collapsed = &mut state
                    .sessions
                    .entry(session_key)
                    .or_default()
                    .collapsed_workspaces;
                if !collapsed.remove(&workspace_id) {
                    collapsed.insert(workspace_id);
                }
            })?;
            restart_for_state_change(&control, &state_dir, &socket)
        }
        CommandKind::ExpandAll => {
            store.update(|state| {
                state
                    .sessions
                    .entry(session_key)
                    .or_default()
                    .collapsed_workspaces
                    .clear();
            })?;
            restart_for_state_change(&control, &state_dir, &socket)
        }
        CommandKind::CollapseAll => {
            let snapshot = HerdrClient::new(socket.clone()).snapshot()?;
            store.update(|state| {
                let collapsed = &mut state
                    .sessions
                    .entry(session_key)
                    .or_default()
                    .collapsed_workspaces;
                collapsed.extend(
                    snapshot
                        .workspaces
                        .into_iter()
                        .map(|workspace| workspace.workspace_id),
                );
            })?;
            restart_for_state_change(&control, &state_dir, &socket)
        }
        CommandKind::Configure => {
            let path = herdr_config_path()?;
            let backup = sidebar_config::configure(&path, &config)?;
            reload_herdr()?;
            println!(
                "configured {}; backup: {}",
                path.display(),
                backup.display()
            );
            Ok(())
        }
        CommandKind::Unconfigure => {
            let _ = notify(&control, ControlCommand::Stop);
            if let Err(error) = clear_owned_metadata(&HerdrClient::new(socket), 14) {
                eprintln!("warning: could not clear live metadata: {error:#}");
            }
            let path = herdr_config_path()?;
            let backup = sidebar_config::unconfigure(&path)?;
            reload_herdr()?;
            match backup {
                Some(path) => println!("unconfigured; backup: {}", path.display()),
                None => println!("herdr-tree was not configured"),
            }
            Ok(())
        }
        CommandKind::Doctor => {
            let snapshot = HerdrClient::new(socket.clone()).snapshot()?;
            let collapsed = collapsed_set(&store, &session_key)?;
            let state = store.read()?;
            let session = state
                .sessions
                .get(&session_key)
                .cloned()
                .unwrap_or_default();
            let tree = project_snapshot(
                &snapshot,
                &collapsed,
                &session.workspace_directories,
                &session.manual_workspace_names,
                &session.manual_tab_names,
            );
            println!("Herdr socket: {}", socket.display());
            println!(
                "plugin config: {}",
                config_dir.join("config.toml").display()
            );
            println!("plugin state: {}", state_dir.display());
            for workspace in &tree.workspaces {
                println!(
                    "workspace {} {:?}: focused={} collapsed={} tabs={}",
                    workspace.workspace_id,
                    workspace.label,
                    workspace.focused,
                    workspace.collapsed,
                    workspace.total_tabs
                );
                for tab in &workspace.tabs {
                    println!(
                        "  tab {} {} {:?}: focused={} status={:?} panes={} agents={} cwd={}",
                        tab.index,
                        tab.tab_id,
                        tab.label,
                        tab.focused,
                        tab.status,
                        tab.pane_count,
                        tab.agents.join(" + "),
                        tab.directory
                            .as_ref()
                            .map_or_else(|| "-".into(), |path| path.display().to_string())
                    );
                }
                for (token, value) in render_workspace(workspace, &config, RenderClock::default()) {
                    if let Some(value) = value {
                        println!("    {token} = {value:?}");
                    }
                }
            }
            println!("note: tab token rows support foreground and bold styling, not backgrounds");
            println!("note: collapsed/mobile sidebar uses Herdr's native compact layout");
            Ok(())
        }
    }
}

fn collapsed_set(store: &StateStore, session_key: &str) -> Result<BTreeSet<String>> {
    Ok(store
        .read()?
        .sessions
        .get(session_key)
        .map(|session| session.collapsed_workspaces.iter().cloned().collect())
        .unwrap_or_default())
}

fn restart_for_state_change(
    control: &std::path::Path,
    state_dir: &std::path::Path,
    herdr_socket: &std::path::Path,
) -> Result<()> {
    let _ = notify(control, ControlCommand::Stop);
    for _ in 0..20 {
        if std::os::unix::net::UnixStream::connect(control).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    start_background(state_dir, herdr_socket)
}

fn reload_herdr() -> Result<()> {
    let binary = std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into());
    let status = Command::new(binary)
        .args(["server", "reload-config"])
        .status()
        .context("failed to run `herdr server reload-config`")?;
    if !status.success() {
        anyhow::bail!("`herdr server reload-config` failed with {status}");
    }
    Ok(())
}

fn clear_owned_metadata(client: &HerdrClient, max_tabs: usize) -> Result<()> {
    let snapshot = client.snapshot()?;
    let empty = empty_workspace_tokens(max_tabs);
    let mut sequence = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    for workspace in snapshot.workspaces {
        for chunk in chunks(&empty) {
            sequence = sequence.wrapping_add(1);
            client.report_metadata(&workspace.workspace_id, sequence, &chunk)?;
        }
    }
    Ok(())
}

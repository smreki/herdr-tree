use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
};

use crate::herdr::{AgentInfo, AgentStatus, SessionSnapshot};

use super::{TabStatus, TabView, TreeView, WorkspaceView};

#[must_use]
pub fn project_snapshot(
    snapshot: &SessionSnapshot,
    collapsed: &BTreeSet<String>,
    workspace_directories: &BTreeMap<String, String>,
    workspace_names: &BTreeMap<String, String>,
    tab_names: &BTreeMap<String, String>,
) -> TreeView {
    let workspaces = snapshot
        .workspaces
        .iter()
        .map(|workspace| {
            let tabs = snapshot
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == workspace.workspace_id)
                .enumerate()
                .map(|(offset, tab)| {
                    let representative = snapshot.representative_pane(&tab.tab_id);
                    let directory = representative
                        .and_then(|pane| pane.foreground_cwd.as_ref().or(pane.cwd.as_ref()))
                        .map(PathBuf::from);
                    let agents: Vec<_> = snapshot.agents_for_tab(&tab.tab_id).collect();
                    TabView {
                        tab_id: tab.tab_id.clone(),
                        index: offset + 1,
                        label: tab_names
                            .get(&tab.tab_id)
                            .cloned()
                            .unwrap_or_else(|| display_name(&tab.label, directory.as_deref())),
                        directory,
                        pane_count: snapshot.tiled_pane_count(&tab.tab_id, tab.pane_count),
                        agents: agent_names(&agents),
                        status: aggregate_status(&agents),
                        focused: tab.focused || workspace.active_tab_id == tab.tab_id,
                        git: None,
                    }
                })
                .collect();
            WorkspaceView {
                workspace_id: workspace.workspace_id.clone(),
                index: workspace.number,
                label: workspace_names
                    .get(&workspace.workspace_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        display_name(
                            &workspace.label,
                            workspace_directories
                                .get(&workspace.workspace_id)
                                .map(std::path::Path::new),
                        )
                    }),
                focused: workspace.focused,
                collapsed: collapsed.contains(&workspace.workspace_id),
                total_tabs: workspace.tab_count,
                tabs,
            }
        })
        .collect();
    TreeView { workspaces }
}

fn display_name(label: &str, directory: Option<&std::path::Path>) -> String {
    directory
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(label)
        .to_owned()
}

fn agent_names(agents: &[&AgentInfo]) -> Vec<String> {
    let mut seen = HashSet::new();
    agents
        .iter()
        .filter_map(|agent| agent.display_agent.as_ref().or(agent.agent.as_ref()))
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect()
}

fn aggregate_status(agents: &[&AgentInfo]) -> TabStatus {
    if agents.is_empty() {
        return TabStatus::NoAgent;
    }
    agents
        .iter()
        .map(|agent| match agent.agent_status {
            AgentStatus::Blocked => TabStatus::Blocked,
            AgentStatus::Working => TabStatus::Working,
            AgentStatus::Done => TabStatus::Done,
            AgentStatus::Idle => TabStatus::Idle,
            AgentStatus::Unknown => TabStatus::Unknown,
        })
        .max()
        .unwrap_or(TabStatus::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(status: AgentStatus, name: &str) -> AgentInfo {
        AgentInfo {
            pane_id: name.into(),
            workspace_id: "w".into(),
            tab_id: "t".into(),
            focused: false,
            cwd: None,
            foreground_cwd: None,
            agent: Some(name.into()),
            display_agent: None,
            agent_status: status,
            state_labels: std::collections::BTreeMap::default(),
        }
    }

    #[test]
    fn blocked_has_precedence_over_working_and_done() {
        let agents = [
            agent(AgentStatus::Done, "one"),
            agent(AgentStatus::Working, "two"),
            agent(AgentStatus::Blocked, "three"),
        ];
        assert_eq!(
            aggregate_status(&agents.iter().collect::<Vec<_>>()),
            TabStatus::Blocked
        );
    }

    #[test]
    fn no_agents_remains_distinct_from_idle() {
        assert_eq!(aggregate_status(&[]), TabStatus::NoAgent);
    }

    #[test]
    fn tab_indexes_are_one_based_within_each_workspace() {
        let snapshot: SessionSnapshot = serde_json::from_value(serde_json::json!({
            "version": "0.9.1",
            "protocol": 22,
            "workspaces": [{
                "workspace_id": "w",
                "number": 4,
                "label": "dashboard",
                "focused": true,
                "tab_count": 1,
                "active_tab_id": "t"
            }],
            "tabs": [{
                "tab_id": "t",
                "workspace_id": "w",
                "number": 11,
                "label": "shell",
                "focused": true,
                "pane_count": 1
            }],
            "panes": [],
            "layouts": [],
            "agents": []
        }))
        .expect("valid snapshot fixture");
        let tree = project_snapshot(
            &snapshot,
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(tree.workspaces[0].tabs[0].index, 1);
    }
}

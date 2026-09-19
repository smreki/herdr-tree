use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Default)]
pub struct SessionSnapshot {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub protocol: u32,
    #[serde(default)]
    pub focused_workspace_id: Option<String>,
    #[serde(default)]
    pub focused_tab_id: Option<String>,
    #[serde(default)]
    pub focused_pane_id: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceInfo>,
    #[serde(default)]
    pub tabs: Vec<TabInfo>,
    #[serde(default)]
    pub panes: Vec<PaneInfo>,
    #[serde(default)]
    pub layouts: Vec<PaneLayout>,
    #[serde(default)]
    pub agents: Vec<AgentInfo>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub tab_count: usize,
    pub active_tab_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TabInfo {
    pub tab_id: String,
    pub workspace_id: String,
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub pane_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub focused: bool,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent_status: AgentStatus,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub focused: bool,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub state_labels: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneLayout {
    pub workspace_id: String,
    pub tab_id: String,
    pub focused_pane_id: String,
    #[serde(default)]
    pub panes: Vec<LayoutPane>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LayoutPane {
    pub pane_id: String,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Working,
    Blocked,
    Idle,
    Done,
    #[default]
    Unknown,
}

impl SessionSnapshot {
    pub fn panes_for_tab(&self, tab_id: &str) -> impl Iterator<Item = &PaneInfo> {
        self.panes.iter().filter(move |pane| pane.tab_id == tab_id)
    }

    pub fn agents_for_tab(&self, tab_id: &str) -> impl Iterator<Item = &AgentInfo> {
        self.agents
            .iter()
            .filter(move |agent| agent.tab_id == tab_id)
    }

    #[must_use]
    pub fn representative_pane(&self, tab_id: &str) -> Option<&PaneInfo> {
        let layout = self.layouts.iter().find(|layout| layout.tab_id == tab_id);
        let selected_id = layout.and_then(|layout| {
            layout
                .panes
                .iter()
                .find(|pane| pane.focused)
                .map(|pane| pane.pane_id.as_str())
                .or(Some(layout.focused_pane_id.as_str()))
        });
        selected_id
            .and_then(|pane_id| self.panes.iter().find(|pane| pane.pane_id == pane_id))
            .or_else(|| self.panes_for_tab(tab_id).find(|pane| pane.focused))
            .or_else(|| {
                layout
                    .and_then(|layout| layout.panes.first())
                    .and_then(|pane| self.panes.iter().find(|item| item.pane_id == pane.pane_id))
            })
            .or_else(|| self.panes_for_tab(tab_id).next())
    }

    #[must_use]
    pub fn tiled_pane_count(&self, tab_id: &str, fallback: usize) -> usize {
        self.layouts
            .iter()
            .find(|layout| layout.tab_id == tab_id)
            .map_or(fallback, |layout| layout.panes.len())
    }
}

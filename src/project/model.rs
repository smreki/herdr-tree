use std::path::PathBuf;

use crate::git::GitStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeView {
    pub workspaces: Vec<WorkspaceView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub index: usize,
    pub label: String,
    pub focused: bool,
    pub collapsed: bool,
    pub total_tabs: usize,
    pub tabs: Vec<TabView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView {
    pub tab_id: String,
    pub index: usize,
    pub label: String,
    pub directory: Option<PathBuf>,
    pub pane_count: usize,
    pub agents: Vec<String>,
    pub status: TabStatus,
    pub focused: bool,
    pub git: Option<GitStatus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabStatus {
    NoAgent,
    Unknown,
    Idle,
    Done,
    Working,
    Blocked,
}

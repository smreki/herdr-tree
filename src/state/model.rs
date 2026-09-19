use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PersistentState {
    pub version: u32,
    pub sessions: BTreeMap<String, SessionState>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionState {
    pub collapsed_workspaces: BTreeSet<String>,
    pub workspace_directories: BTreeMap<String, String>,
    pub manual_workspace_names: BTreeMap<String, String>,
    pub manual_tab_names: BTreeMap<String, String>,
    pub held_done_tabs: BTreeSet<String>,
    pub held_blocked_tabs: BTreeSet<String>,
}

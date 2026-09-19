use std::collections::BTreeMap;

use crate::{
    config::{Config, Layout, PaneCountMode, WorkspaceTabCount},
    herdr::TokenDelta,
    project::{TabStatus, WorkspaceView},
};

use super::format_git;

const INDENT: &str = "\u{200b}";
const DETAIL_INDENT: &str = "\u{200b}    ";

pub type WorkspaceTokens = BTreeMap<String, Option<String>>;

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderClock {
    pub spinner_frame: usize,
    pub blocked_quiet: bool,
}

#[must_use]
pub fn render_workspace(
    workspace: &WorkspaceView,
    config: &Config,
    clock: RenderClock,
) -> WorkspaceTokens {
    let mut tokens = empty_workspace_tokens(config.max_tabs);
    let count = match config.workspace_tab_count {
        WorkspaceTabCount::Always => Some(workspace.total_tabs),
        WorkspaceTabCount::Collapsed if workspace.collapsed => Some(workspace.total_tabs),
        WorkspaceTabCount::Never | WorkspaceTabCount::Collapsed => None,
    };
    let suffix = count.map_or_else(String::new, |value| format!(" ({value})"));
    // The disclosure and the name are separate cells that Herdr joins without
    // padding, so the disclosure carries the gap.
    let disclosure = if workspace.collapsed { "▸ " } else { "▾ " };
    tokens.insert("tree_toggle".into(), Some(disclosure.into()));
    tokens.insert(
        "tree_workspace".into(),
        Some(format!("{} {}{suffix}", workspace.index, workspace.label)),
    );

    if workspace.collapsed {
        return tokens;
    }
    for (slot, tab) in workspace.tabs.iter().take(config.max_tabs).enumerate() {
        let glyph = glyph(tab.status, config, clock);
        let agents = if tab.agents.is_empty() {
            String::new()
        } else {
            format!("{} - ", tab.agents.join(" + "))
        };
        let panes = pane_count(tab.pane_count, config.show_pane_count);
        // Herdr's own tab labels already carry the tab number, so the row
        // shows the label as-is instead of repeating the index.
        let value = format!("{INDENT}{glyph} {agents}{}{panes}", tab.label);
        let state = state_name(tab.status);
        let active_suffix = if tab.focused { "_active" } else { "" };
        tokens.insert(
            format!("tree_tab_{}_{}{}", slot + 1, state, active_suffix),
            Some(value),
        );
        if config.layout == Layout::Detailed {
            tokens.insert(
                format!("tree_git_{}", slot + 1),
                tab.git
                    .as_ref()
                    .map(|git| format!("{DETAIL_INDENT}{}", format_git(git, config.git_detail))),
            );
        }
    }
    let hidden = workspace.tabs.len().saturating_sub(config.max_tabs);
    if hidden > 0 {
        tokens.insert(
            "tree_overflow".into(),
            Some(format!("{INDENT}… {hidden} more tabs")),
        );
    }
    tokens
}

#[must_use]
pub fn diff_tokens(previous: &WorkspaceTokens, next: &WorkspaceTokens) -> TokenDelta {
    next.iter()
        .filter(|(key, value)| previous.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .chain(
            previous
                .keys()
                .filter(|key| !next.contains_key(*key))
                .map(|key| (key.clone(), None)),
        )
        .collect()
}

#[must_use]
pub fn empty_workspace_tokens(max_tabs: usize) -> WorkspaceTokens {
    let mut tokens = BTreeMap::new();
    tokens.insert("tree_toggle".into(), None);
    tokens.insert("tree_workspace".into(), None);
    tokens.insert("tree_overflow".into(), None);
    for slot in 1..=max_tabs {
        for state in ["working", "done", "blocked", "idle", "unknown"] {
            tokens.insert(format!("tree_tab_{slot}_{state}"), None);
            tokens.insert(format!("tree_tab_{slot}_{state}_active"), None);
        }
        tokens.insert(format!("tree_git_{slot}"), None);
    }
    tokens
}

fn pane_count(count: usize, mode: PaneCountMode) -> String {
    match mode {
        PaneCountMode::Always => format!(" ({count})"),
        PaneCountMode::Multiple if count > 1 => format!(" ({count})"),
        PaneCountMode::Never | PaneCountMode::Multiple => String::new(),
    }
}

fn state_name(status: TabStatus) -> &'static str {
    match status {
        TabStatus::Working => "working",
        TabStatus::Done => "done",
        TabStatus::Blocked => "blocked",
        TabStatus::Idle | TabStatus::NoAgent => "idle",
        TabStatus::Unknown => "unknown",
    }
}

fn glyph(status: TabStatus, config: &Config, clock: RenderClock) -> &str {
    match status {
        TabStatus::Working => {
            let index = clock.spinner_frame % config.glyphs.working.len();
            &config.glyphs.working[index]
        }
        TabStatus::Done => &config.glyphs.done,
        TabStatus::Blocked if clock.blocked_quiet => &config.glyphs.blocked_quiet,
        TabStatus::Blocked => &config.glyphs.blocked,
        TabStatus::Idle => &config.glyphs.idle,
        TabStatus::NoAgent => &config.glyphs.no_agent,
        TabStatus::Unknown => &config.glyphs.unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::git::{GitStatus, Head};
    use crate::project::TabView;

    fn workspace(status: TabStatus, focused: bool) -> WorkspaceView {
        WorkspaceView {
            workspace_id: "w".into(),
            index: 2,
            label: "dashboard".into(),
            focused: true,
            collapsed: false,
            total_tabs: 1,
            tabs: vec![TabView {
                tab_id: "t".into(),
                index: 1,
                label: "dir_name".into(),
                directory: Some(PathBuf::from("/tmp/dir_name")),
                pane_count: 2,
                agents: vec!["codex".into()],
                status,
                focused,
                git: None,
            }],
        }
    }

    #[test]
    fn active_working_tab_uses_spinner_without_marker() {
        let tokens = render_workspace(
            &workspace(TabStatus::Working, true),
            &Config::default(),
            RenderClock {
                spinner_frame: 2,
                blocked_quiet: false,
            },
        );
        assert_eq!(
            tokens["tree_tab_1_working_active"].as_deref(),
            Some("\u{200b}⣟ codex - dir_name (2)")
        );
    }

    #[test]
    fn agent_and_shell_tabs_use_distinct_idle_glyphs() {
        let config = Config::default();
        let mut agent = workspace(TabStatus::Idle, false);
        agent.tabs[0].pane_count = 1;
        let tokens = render_workspace(&agent, &config, RenderClock::default());
        assert_eq!(
            tokens["tree_tab_1_idle"].as_deref(),
            Some("\u{200b}◉ codex - dir_name")
        );

        let mut shell = workspace(TabStatus::NoAgent, false);
        shell.tabs[0].pane_count = 1;
        shell.tabs[0].agents.clear();
        let tokens = render_workspace(&shell, &config, RenderClock::default());
        assert_eq!(
            tokens["tree_tab_1_idle"].as_deref(),
            Some("\u{200b}○ dir_name")
        );
    }

    #[test]
    fn collapse_clears_children_and_adds_tab_count() {
        let mut workspace = workspace(TabStatus::Idle, false);
        workspace.collapsed = true;
        let tokens = render_workspace(&workspace, &Config::default(), RenderClock::default());
        assert_eq!(tokens["tree_toggle"].as_deref(), Some("▸ "));
        assert_eq!(tokens["tree_workspace"].as_deref(), Some("2 dashboard (1)"));
        assert!(tokens["tree_tab_1_idle"].is_none());
    }

    #[test]
    fn delta_contains_only_changed_tokens() {
        let before = render_workspace(
            &workspace(TabStatus::Working, false),
            &Config::default(),
            RenderClock::default(),
        );
        let after = render_workspace(
            &workspace(TabStatus::Working, false),
            &Config::default(),
            RenderClock {
                spinner_frame: 1,
                blocked_quiet: false,
            },
        );
        let delta = diff_tokens(&before, &after);
        assert_eq!(delta.len(), 1);
    }

    #[test]
    fn git_detail_aligns_with_tab_index() {
        let mut workspace = workspace(TabStatus::Idle, false);
        workspace.tabs[0].git = Some(GitStatus {
            repository: "/tmp/repository".into(),
            head: Head::Branch("main".into()),
            ahead: 0,
            behind: 0,
            stashes: 0,
            staged: 0,
            unstaged: 0,
            untracked: 0,
            conflicted: 0,
            dirty_paths: 0,
        });
        let tokens = render_workspace(&workspace, &Config::default(), RenderClock::default());
        assert_eq!(tokens["tree_git_1"].as_deref(), Some("\u{200b}    main"));
    }
}

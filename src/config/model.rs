use serde::{Deserialize, Serialize};

/// Plugin configuration, parsed strictly from `config.toml`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub layout: Layout,
    pub max_tabs: usize,
    pub show_pane_count: PaneCountMode,
    pub workspace_tab_count: WorkspaceTabCount,
    pub git_detail: GitDetail,
    pub git_refresh_interval_seconds: u64,
    pub git_background_refresh_interval_seconds: u64,
    pub git_command_timeout_seconds: u64,
    pub spinner_interval_ms: u64,
    pub done_hold: DoneHold,
    pub done_hold_seconds: u64,
    pub blocked_hold: bool,
    pub idle_grace_seconds: f64,
    pub glyphs: Glyphs,
    pub colors: Colors,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Detailed,
    Compact,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaneCountMode {
    Never,
    Always,
    Multiple,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTabCount {
    Never,
    Collapsed,
    Always,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitDetail {
    Compact,
    Detailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoneHold {
    UntilSeen,
    Seconds,
    Off,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Glyphs {
    pub done: String,
    pub blocked: String,
    pub blocked_quiet: String,
    pub idle: String,
    pub no_agent: String,
    pub unknown: String,
    pub working: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub active_tab: String,
    pub normal_tab: String,
    pub detail: String,
    pub done: String,
    pub blocked: String,
    pub idle: String,
    pub unknown: String,
    pub active_workspace_bg: String,
    pub selection_bg: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: Layout::Detailed,
            max_tabs: 7,
            show_pane_count: PaneCountMode::Multiple,
            workspace_tab_count: WorkspaceTabCount::Collapsed,
            git_detail: GitDetail::Compact,
            git_refresh_interval_seconds: 2,
            git_background_refresh_interval_seconds: 5,
            git_command_timeout_seconds: 3,
            spinner_interval_ms: 150,
            done_hold: DoneHold::UntilSeen,
            done_hold_seconds: 6,
            blocked_hold: true,
            idle_grace_seconds: 0.5,
            glyphs: Glyphs::default(),
            colors: Colors::default(),
        }
    }
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            done: "✓".into(),
            blocked: "!".into(),
            blocked_quiet: "·".into(),
            idle: "◉".into(),
            no_agent: "○".into(),
            unknown: "◌".into(),
            working: ["⣷", "⣯", "⣟", "⡿", "⢿", "⣻", "⣽", "⣾"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            active_tab: "#89b4fa".into(),
            normal_tab: "#cdd6f4".into(),
            detail: "#a6adc8".into(),
            done: "#a6e3a1".into(),
            blocked: "#f38ba8".into(),
            idle: "#a6adc8".into(),
            unknown: "#cba6f7".into(),
            active_workspace_bg: "#313244".into(),
            selection_bg: "#45475a".into(),
        }
    }
}

impl Config {
    /// Validate invariants constrained by Herdr's sidebar and metadata APIs.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_tabs == 0 {
            return Err("max_tabs must be positive".into());
        }
        let max = match self.layout {
            Layout::Detailed => 7,
            Layout::Compact => 14,
        };
        if self.max_tabs > max {
            return Err(format!(
                "max_tabs cannot exceed {max} for {:?} layout",
                self.layout
            ));
        }
        if self.git_refresh_interval_seconds == 0
            || self.git_background_refresh_interval_seconds == 0
            || self.git_command_timeout_seconds == 0
            || self.spinner_interval_ms == 0
        {
            return Err("refresh intervals and timeouts must be positive".into());
        }
        if !self.idle_grace_seconds.is_finite() || self.idle_grace_seconds < 0.0 {
            return Err("idle_grace_seconds must be finite and non-negative".into());
        }
        let glyphs = [
            &self.glyphs.done,
            &self.glyphs.blocked,
            &self.glyphs.blocked_quiet,
            &self.glyphs.idle,
            &self.glyphs.no_agent,
            &self.glyphs.unknown,
        ];
        if glyphs.iter().any(|glyph| glyph.is_empty()) || self.glyphs.working.is_empty() {
            return Err("glyphs must not be empty".into());
        }
        if self.glyphs.working.iter().any(String::is_empty) {
            return Err("working glyphs must not be empty".into());
        }
        for color in [
            &self.colors.active_tab,
            &self.colors.normal_tab,
            &self.colors.detail,
            &self.colors.done,
            &self.colors.blocked,
            &self.colors.idle,
            &self.colors.unknown,
            &self.colors.active_workspace_bg,
            &self.colors.selection_bg,
        ] {
            if !valid_hex_color(color) {
                return Err(format!("invalid color {color:?}; expected #RGB or #RRGGBB"));
            }
        }
        Ok(())
    }
}

fn valid_hex_color(value: &str) -> bool {
    matches!(value.len(), 4 | 7)
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_use_multiple_pane_counts() {
        let config = Config::default();
        assert_eq!(config.show_pane_count, PaneCountMode::Multiple);
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn detailed_layout_rejects_more_than_seven_tabs() {
        let config = Config {
            max_tabs: 8,
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }
}

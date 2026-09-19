use crate::{config::Config, project::TabStatus};

const SIDEBAR_START: &str = "# >>> herdr-tree managed sidebar >>>";
const SIDEBAR_END: &str = "# <<< herdr-tree managed sidebar <<<";
const THEME_START: &str = "# >>> herdr-tree managed theme >>>";
const THEME_END: &str = "# <<< herdr-tree managed theme <<<";

#[must_use]
pub fn render_managed_blocks(config: &Config) -> String {
    format!("{}\n\n{}", render_theme(config), render_sidebar(config))
}

#[must_use]
pub fn render_theme(config: &Config) -> String {
    format!(
        "{THEME_START}\n[theme.custom]\nactive_row_bg = {:?}\nselection_bg = {:?}\n{THEME_END}",
        config.colors.active_workspace_bg, config.colors.selection_bg
    )
}

#[must_use]
pub fn render_sidebar(config: &Config) -> String {
    let mut rows = Vec::with_capacity(config.max_tabs * 2 + 2);
    rows.push(format!(
        "[{}, {}]",
        cell("$tree_toggle", &config.colors.normal_tab, true, false),
        cell("$tree_workspace", &config.colors.normal_tab, true, false)
    ));
    for slot in 1..=config.max_tabs {
        let mut cells = Vec::with_capacity(10);
        for status in [
            TabStatus::Working,
            TabStatus::Done,
            TabStatus::Blocked,
            TabStatus::Idle,
            TabStatus::Unknown,
        ] {
            let name = status_name(status);
            let color = status_color(status, config);
            cells.push(cell(
                &format!("$tree_tab_{slot}_{name}"),
                color,
                false,
                false,
            ));
            cells.push(cell(
                &format!("$tree_tab_{slot}_{name}_active"),
                &config.colors.active_tab,
                true,
                false,
            ));
        }
        rows.push(format!("[{}]", cells.join(", ")));
        if config.layout == crate::config::Layout::Detailed {
            rows.push(format!(
                "[{}]",
                cell(
                    &format!("$tree_git_{slot}"),
                    &config.colors.detail,
                    false,
                    true
                )
            ));
        }
    }
    rows.push(format!(
        "[{}]",
        cell("$tree_overflow", &config.colors.detail, false, true)
    ));
    format!(
        "{SIDEBAR_START}\n[ui.sidebar.spaces]\nrow_gap = 0\nrows = [\n  {}\n]\n{SIDEBAR_END}",
        rows.join(",\n  ")
    )
}

fn cell(token: &str, color: &str, bold: bool, dim: bool) -> String {
    format!("{{ token = {token:?}, fg = {color:?}, bold = {bold}, dim = {dim} }}")
}

fn status_name(status: TabStatus) -> &'static str {
    match status {
        TabStatus::Working => "working",
        TabStatus::Done => "done",
        TabStatus::Blocked => "blocked",
        TabStatus::Idle | TabStatus::NoAgent => "idle",
        TabStatus::Unknown => "unknown",
    }
}

fn status_color(status: TabStatus, config: &Config) -> &str {
    match status {
        TabStatus::Working => &config.colors.normal_tab,
        TabStatus::Done => &config.colors.done,
        TabStatus::Blocked => &config.colors.blocked,
        TabStatus::Idle | TabStatus::NoAgent => &config.colors.idle,
        TabStatus::Unknown => &config.colors.unknown,
    }
}

pub(crate) const MARKERS: [(&str, &str); 2] =
    [(THEME_START, THEME_END), (SIDEBAR_START, SIDEBAR_END)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_default_fits_sixteen_sidebar_rows() {
        let block = render_sidebar(&Config::default());
        let rows = block
            .lines()
            .filter(|line| line.trim_start().starts_with('['))
            .count();
        // The table header is included in this textual count.
        assert_eq!(rows, 17);
        assert!(block.contains("$tree_tab_7_working_active"));
        // Only fields every supported Herdr build accepts may appear in a cell.
        assert!(!block.contains("on_click"));
        assert!(!block.contains("separator"));
    }
}

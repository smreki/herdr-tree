use std::{
    collections::{BTreeSet, HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::config::{Config, DoneHold};

use super::{TabStatus, TreeView};

#[derive(Default)]
pub struct LifecycleTracker {
    entries: HashMap<String, Entry>,
}

struct Entry {
    previous: TabStatus,
    held: Option<Held>,
    done_acknowledged: bool,
    grace_started: Option<Instant>,
}

enum Held {
    Done(Instant),
    Blocked,
}

impl LifecycleTracker {
    #[must_use]
    pub fn from_held(done: &BTreeSet<String>, blocked: &BTreeSet<String>, now: Instant) -> Self {
        let mut entries = HashMap::new();
        entries.extend(done.iter().map(|tab_id| {
            (
                tab_id.clone(),
                Entry {
                    previous: TabStatus::Unknown,
                    held: Some(Held::Done(now)),
                    done_acknowledged: false,
                    grace_started: None,
                },
            )
        }));
        entries.extend(blocked.iter().map(|tab_id| {
            (
                tab_id.clone(),
                Entry {
                    previous: TabStatus::Unknown,
                    held: Some(Held::Blocked),
                    done_acknowledged: false,
                    grace_started: None,
                },
            )
        }));
        Self { entries }
    }

    /// Apply done/blocked retention to a freshly projected authoritative tree.
    pub fn apply(&mut self, tree: &mut TreeView, config: &Config, now: Instant) {
        let mut live = HashSet::new();
        for workspace in &mut tree.workspaces {
            for tab in &mut workspace.tabs {
                live.insert(tab.tab_id.clone());
                let raw = tab.status;
                let entry = self.entries.entry(tab.tab_id.clone()).or_insert(Entry {
                    previous: raw,
                    held: None,
                    done_acknowledged: false,
                    grace_started: None,
                });
                if raw == TabStatus::Working {
                    entry.held = None;
                    entry.done_acknowledged = false;
                    entry.grace_started = None;
                } else if raw == TabStatus::Blocked && config.blocked_hold {
                    entry.held = Some(Held::Blocked);
                    entry.grace_started = None;
                } else if raw == TabStatus::Done
                    && !entry.done_acknowledged
                    && !matches!(entry.held, Some(Held::Done(_)))
                {
                    entry.done_acknowledged = false;
                    entry.grace_started = None;
                    entry.held = match config.done_hold {
                        DoneHold::Off => None,
                        DoneHold::UntilSeen | DoneHold::Seconds => Some(Held::Done(now)),
                    };
                } else if entry.previous == TabStatus::Working
                    && matches!(
                        raw,
                        TabStatus::Idle | TabStatus::Unknown | TabStatus::NoAgent
                    )
                {
                    entry.grace_started = Some(now);
                }

                if let Some(started) = entry.grace_started {
                    let grace = Duration::try_from_secs_f64(config.idle_grace_seconds)
                        .unwrap_or(Duration::MAX);
                    if now.duration_since(started) < grace {
                        tab.status = TabStatus::Working;
                        entry.previous = raw;
                        continue;
                    }
                    entry.grace_started = None;
                    entry.done_acknowledged = false;
                    entry.held = match config.done_hold {
                        DoneHold::Off => None,
                        DoneHold::UntilSeen | DoneHold::Seconds => Some(Held::Done(now)),
                    };
                }

                if tab.focused && matches!(entry.held, Some(Held::Done(_))) {
                    entry.held = None;
                    entry.done_acknowledged = true;
                    tab.status = if raw == TabStatus::Done {
                        TabStatus::Idle
                    } else {
                        raw
                    };
                } else {
                    if matches!(entry.held, Some(Held::Done(since))
                        if config.done_hold == DoneHold::Seconds
                            && now.duration_since(since)
                                >= Duration::from_secs(config.done_hold_seconds))
                    {
                        entry.held = None;
                        entry.done_acknowledged = true;
                    }
                    tab.status = match entry.held {
                        Some(Held::Done(_)) => TabStatus::Done,
                        Some(Held::Blocked) => TabStatus::Blocked,
                        None if raw == TabStatus::Done && entry.done_acknowledged => {
                            TabStatus::Idle
                        }
                        None => raw,
                    };
                }
                entry.previous = raw;
            }
        }
        self.entries.retain(|tab_id, _| live.contains(tab_id));
    }

    #[must_use]
    pub fn held(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut done = BTreeSet::new();
        let mut blocked = BTreeSet::new();
        for (tab_id, entry) in &self.entries {
            match entry.held {
                Some(Held::Done(_)) => {
                    done.insert(tab_id.clone());
                }
                Some(Held::Blocked) => {
                    blocked.insert(tab_id.clone());
                }
                None => {}
            }
        }
        (done, blocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{TabView, WorkspaceView};

    fn tree(status: TabStatus, focused: bool) -> TreeView {
        TreeView {
            workspaces: vec![WorkspaceView {
                workspace_id: "w".into(),
                index: 1,
                label: "workspace".into(),
                focused,
                collapsed: false,
                total_tabs: 1,
                tabs: vec![TabView {
                    tab_id: "t".into(),
                    index: 1,
                    label: "tab".into(),
                    directory: None,
                    pane_count: 1,
                    agents: vec!["codex".into()],
                    status,
                    focused,
                    git: None,
                }],
            }],
        }
    }

    #[test]
    fn working_to_idle_holds_done_until_focus() {
        let config = Config::default();
        let now = Instant::now();
        let mut tracker = LifecycleTracker::default();
        let mut working = tree(TabStatus::Working, false);
        tracker.apply(&mut working, &config, now);
        let mut idle = tree(TabStatus::Idle, false);
        tracker.apply(&mut idle, &config, now);
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Working);
        let mut idle = tree(TabStatus::Idle, false);
        tracker.apply(&mut idle, &config, now + Duration::from_secs(3));
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Done);
        let mut focused = tree(TabStatus::Idle, true);
        tracker.apply(&mut focused, &config, now);
        assert_eq!(focused.workspaces[0].tabs[0].status, TabStatus::Idle);
    }

    #[test]
    fn blocked_holds_until_work_resumes() {
        let config = Config::default();
        let now = Instant::now();
        let mut tracker = LifecycleTracker::default();
        let mut blocked = tree(TabStatus::Blocked, false);
        tracker.apply(&mut blocked, &config, now);
        let mut idle = tree(TabStatus::Idle, false);
        tracker.apply(&mut idle, &config, now);
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Blocked);
        let mut working = tree(TabStatus::Working, false);
        tracker.apply(&mut working, &config, now);
        assert_eq!(working.workspaces[0].tabs[0].status, TabStatus::Working);
    }

    #[test]
    fn timed_done_hold_expires() {
        let config = Config {
            done_hold: DoneHold::Seconds,
            done_hold_seconds: 1,
            idle_grace_seconds: 0.0,
            ..Config::default()
        };
        let now = Instant::now();
        let mut tracker = LifecycleTracker::default();
        tracker.apply(&mut tree(TabStatus::Working, false), &config, now);
        let mut idle = tree(TabStatus::Idle, false);
        tracker.apply(&mut idle, &config, now);
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Done);
        tracker.apply(&mut idle, &config, now + Duration::from_secs(2));
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Idle);
    }

    #[test]
    fn held_state_can_be_restored_after_restart() {
        let done = BTreeSet::from(["t".to_owned()]);
        let mut tracker = LifecycleTracker::from_held(&done, &BTreeSet::new(), Instant::now());
        let mut idle = tree(TabStatus::Idle, false);
        tracker.apply(&mut idle, &Config::default(), Instant::now());
        assert_eq!(idle.workspaces[0].tabs[0].status, TabStatus::Done);
    }
}

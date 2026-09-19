use crate::{
    config::GitDetail,
    git::{GitStatus, Head},
};

#[must_use]
pub fn format_git(status: &GitStatus, detail: GitDetail) -> String {
    let mut parts = Vec::new();
    match &status.head {
        Head::Branch(branch) => parts.push(branch.clone()),
        Head::Detached(oid) => parts.push(format!("@{oid}")),
        Head::Unknown => parts.push("@unknown".into()),
    }
    if status.ahead > 0 && status.behind > 0 {
        parts.push(format!("↕{}/{}", status.behind, status.ahead));
    } else {
        if status.behind > 0 {
            parts.push(format!("↓{}", status.behind));
        }
        if status.ahead > 0 {
            parts.push(format!("↑{}", status.ahead));
        }
    }
    if status.stashes > 0 {
        parts.push(format!("▣{}", status.stashes));
    }
    match detail {
        GitDetail::Compact => {
            if status.dirty() > 0 {
                parts.push(format!("✱{}", status.dirty()));
            }
        }
        GitDetail::Detailed => {
            if status.staged > 0 {
                parts.push(format!("●{}", status.staged));
            }
            if status.unstaged > 0 {
                parts.push(format!("✱{}", status.unstaged));
            }
            if status.untracked > 0 {
                parts.push(format!("?{}", status.untracked));
            }
            if status.conflicted > 0 {
                parts.push(format!("!{}", status.conflicted));
            }
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> GitStatus {
        GitStatus {
            repository: "/repo".into(),
            head: Head::Branch("feature/auth".into()),
            ahead: 2,
            behind: 0,
            stashes: 1,
            staged: 2,
            unstaged: 1,
            untracked: 3,
            conflicted: 0,
            dirty_paths: 6,
        }
    }

    #[test]
    fn detailed_status_matches_visual_contract() {
        assert_eq!(
            format_git(&status(), GitDetail::Detailed),
            "feature/auth ↑2 ▣1 ●2 ✱1 ?3"
        );
    }

    #[test]
    fn diverged_status_uses_single_bidirectional_glyph() {
        let mut status = status();
        status.behind = 4;
        assert!(format_git(&status, GitDetail::Compact).contains("↕4/2"));
    }
}

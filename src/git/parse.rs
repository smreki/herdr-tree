use thiserror::Error;

use super::{GitStatus, Head};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("invalid UTF-8 in Git status output")]
    Utf8,
    #[error("invalid branch.ab header: {0}")]
    AheadBehind(String),
}

pub fn parse_porcelain_v2(repository: String, output: &[u8]) -> Result<GitStatus, ParseError> {
    let mut status = GitStatus {
        repository,
        head: Head::Unknown,
        ahead: 0,
        behind: 0,
        stashes: 0,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        conflicted: 0,
        dirty_paths: 0,
    };
    let mut oid = None;
    let mut records = output.split(|byte| *byte == 0).peekable();
    while let Some(bytes) = records.next() {
        if bytes.is_empty() {
            continue;
        }
        let record = std::str::from_utf8(bytes).map_err(|_| ParseError::Utf8)?;
        if let Some(value) = record.strip_prefix("# branch.head ") {
            status.head = if value == "(detached)" {
                Head::Unknown
            } else {
                Head::Branch(value.to_owned())
            };
        } else if let Some(value) = record.strip_prefix("# branch.oid ") {
            oid = Some(value.to_owned());
        } else if let Some(value) = record.strip_prefix("# branch.ab ") {
            let mut parts = value.split_whitespace();
            status.ahead = parse_count(parts.next(), '+', value)?;
            status.behind = parse_count(parts.next(), '-', value)?;
        } else if let Some(value) = record.strip_prefix("# stash ") {
            status.stashes = value.parse().unwrap_or(0);
        } else if record.starts_with("1 ") || record.starts_with("2 ") {
            status.dirty_paths += 1;
            count_xy(&mut status, record.as_bytes().get(2..4));
            if record.starts_with("2 ") {
                let _ = records.next();
            }
        } else if record.starts_with("u ") {
            status.conflicted += 1;
            status.dirty_paths += 1;
        } else if record.starts_with("? ") {
            status.untracked += 1;
            status.dirty_paths += 1;
        }
    }
    if matches!(status.head, Head::Unknown) {
        if let Some(oid) = oid.filter(|value| value != "(initial)") {
            status.head = Head::Detached(oid.chars().take(7).collect());
        }
    }
    Ok(status)
}

fn parse_count(value: Option<&str>, prefix: char, line: &str) -> Result<usize, ParseError> {
    value
        .and_then(|part| part.strip_prefix(prefix))
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| ParseError::AheadBehind(line.to_owned()))
}

fn count_xy(status: &mut GitStatus, xy: Option<&[u8]>) {
    let Some([index, worktree]) = xy else {
        return;
    };
    if *index != b'.' {
        status.staged += 1;
    }
    if *worktree != b'.' {
        status.unstaged += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_divergence_stash_and_all_path_kinds() {
        let input = b"# branch.oid abcdef012345\0# branch.head feature/auth\0# branch.ab +2 -3\0# stash 1\0\
1 M. N... 100644 100644 100644 a b staged\0\
1 .M N... 100644 100644 100644 a b unstaged\0\
2 R. N... 100644 100644 100644 a b R100 renamed\0old\0\
u UU N... 100644 100644 100644 100644 a b c conflict\0? untracked\0! ignored\0";
        let status = parse_porcelain_v2("/repo".into(), input).expect("valid status");
        assert_eq!(status.head, Head::Branch("feature/auth".into()));
        assert_eq!((status.ahead, status.behind, status.stashes), (2, 3, 1));
        assert_eq!(
            (
                status.staged,
                status.unstaged,
                status.untracked,
                status.conflicted
            ),
            (2, 1, 1, 1)
        );
        assert_eq!(status.dirty(), 5);
    }

    #[test]
    fn detached_head_uses_short_oid() {
        let input = b"# branch.oid abcdef012345\0# branch.head (detached)\0";
        let status = parse_porcelain_v2("/repo".into(), input).expect("valid status");
        assert_eq!(status.head, Head::Detached("abcdef0".into()));
    }
}

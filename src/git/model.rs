#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached(String),
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitStatus {
    pub repository: String,
    pub head: Head,
    pub ahead: usize,
    pub behind: usize,
    pub stashes: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
    pub dirty_paths: usize,
}

impl GitStatus {
    #[must_use]
    pub const fn dirty(&self) -> usize {
        self.dirty_paths
    }
}

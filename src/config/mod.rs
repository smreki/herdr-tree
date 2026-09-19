mod loading;
mod model;

pub use loading::{ConfigError, load_or_seed};
pub use model::{Config, DoneHold, GitDetail, Layout, PaneCountMode, WorkspaceTabCount};

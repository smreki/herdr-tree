mod managed;
mod template;

pub use managed::{ManagedConfigError, configure, unconfigure};
pub use template::render_managed_blocks;

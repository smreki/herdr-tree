use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn environment_path(name: &str) -> Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("{name} is unavailable; run this command through Herdr"))
}

pub fn socket_path() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("HERDR_SOCKET_PATH") {
        return Ok(PathBuf::from(value));
    }
    Ok(herdr_config_path()?.with_file_name("herdr.sock"))
}

pub fn herdr_config_path() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("HERDR_CONFIG_PATH") {
        return Ok(PathBuf::from(value));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("cannot resolve Herdr config path")?;
    Ok(base.join("herdr/config.toml"))
}

pub fn seed_shell_runtime_paths(config_dir: &Path, state_dir: &Path) -> Result<()> {
    let executable = std::env::current_exe().context("failed to resolve plugin executable")?;
    let contents = format!("{}\n{}\n", executable.display(), state_dir.display());
    let destination = config_dir.join("runtime-paths");
    let temporary = config_dir.join(format!("runtime-paths.{}.tmp", std::process::id()));
    fs::write(&temporary, contents)
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    fs::rename(&temporary, &destination)
        .with_context(|| format!("failed to replace {}", destination.display()))?;
    Ok(())
}

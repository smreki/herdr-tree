use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use super::{GitStatus, parse_porcelain_v2};

#[derive(Default)]
pub struct GitCache {
    entries: HashMap<PathBuf, CacheEntry>,
    roots: HashMap<PathBuf, PathBuf>,
}

struct CacheEntry {
    status: GitStatus,
    checked_at: Instant,
}

impl GitCache {
    pub fn status(
        &mut self,
        directory: &Path,
        max_age: Duration,
        timeout: Duration,
    ) -> Result<Option<GitStatus>> {
        let known_root = self.roots.get(directory).cloned();
        let root = match known_root.map_or_else(
            || repository_root(directory, timeout),
            |root| Ok(Some(root)),
        ) {
            Ok(Some(root)) => {
                self.roots.insert(directory.to_path_buf(), root.clone());
                root
            }
            Ok(None) => return Ok(None),
            Err(error) => {
                return self
                    .cached_for_directory(directory)
                    .map_or(Err(error), |status| Ok(Some(status)));
            }
        };
        if let Some(entry) = self.entries.get(&root) {
            if entry.checked_at.elapsed() < max_age {
                return Ok(Some(entry.status.clone()));
            }
        }
        let refresh = run_git(
            &root,
            &["status", "--porcelain=v2", "--branch", "--show-stash", "-z"],
            timeout,
        )
        .and_then(|output| {
            parse_porcelain_v2(root.display().to_string(), &output).map_err(Into::into)
        });
        let status = match refresh {
            Ok(status) => status,
            Err(error) => {
                return self
                    .entries
                    .get(&root)
                    .map_or(Err(error), |entry| Ok(Some(entry.status.clone())));
            }
        };
        self.entries.insert(
            root,
            CacheEntry {
                status: status.clone(),
                checked_at: Instant::now(),
            },
        );
        Ok(Some(status))
    }

    fn cached_for_directory(&self, directory: &Path) -> Option<GitStatus> {
        self.roots
            .get(directory)
            .and_then(|root| self.entries.get(root))
            .map(|entry| entry.status.clone())
    }
}

fn repository_root(directory: &Path, timeout: Duration) -> Result<Option<PathBuf>> {
    let output = match run_git(directory, &["rev-parse", "--show-toplevel"], timeout) {
        Ok(output) => output,
        Err(error) if error.to_string().contains("exited with") => return Ok(None),
        Err(error) => return Err(error),
    };
    let path = String::from_utf8(output).context("Git repository path is not UTF-8")?;
    let root = path.trim();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(root)))
}

fn run_git(directory: &Path, arguments: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .args(["-C"])
        .arg(directory)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to run Git in {}", directory.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .context("failed to capture Git output")?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            let _ = child.wait();
            let _ = reader.join();
            bail!("Git command timed out after {} ms", timeout.as_millis());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Git output reader panicked"))??;
    ensure_success(status, directory)?;
    Ok(output)
}

fn ensure_success(status: ExitStatus, directory: &Path) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!(
            "Git command exited with {status} in {}",
            directory.display()
        )
    }
}

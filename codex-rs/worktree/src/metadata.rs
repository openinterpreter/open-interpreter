//! Reads and binds Desktop-compatible thread ownership for managed worktrees.
//!
//! Stores a versioned `codex-thread.json` record in each worktree's Git metadata.
//! Binding publishes the record atomically without replacing another owner;
//! binding the same thread again leaves the existing record unchanged.

use crate::git::git_path;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tempfile::NamedTempFile;

const OWNER_FILENAME: &str = "codex-thread.json";
const OWNER_VERSION: u8 = 1;

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerRecord {
    version: u8,
    owner_thread_id: String,
}

pub(crate) fn owner(checkout: &Path) -> Result<Option<String>> {
    let path = metadata_path(checkout)?;
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot read worktree owner at {}", path.display()));
        }
    };

    let record: OwnerRecord = serde_json::from_slice(&contents)
        .with_context(|| format!("invalid worktree owner at {}", path.display()))?;
    if record.version != OWNER_VERSION || record.owner_thread_id.is_empty() {
        bail!("invalid worktree owner at {}", path.display());
    }

    Ok(Some(record.owner_thread_id))
}

pub(crate) fn bind_thread(checkout: &Path, thread_id: &str) -> Result<()> {
    if thread_id.is_empty() {
        bail!("worktree owner thread ID cannot be empty");
    }

    if let Some(existing) = owner(checkout)? {
        if existing == thread_id {
            return Ok(());
        }
        bail!("worktree already belongs to thread {existing}");
    }

    let path = metadata_path(checkout)?;
    let directory = path.parent().context("worktree metadata has no parent")?;
    let mut temporary = NamedTempFile::new_in(directory)
        .with_context(|| format!("cannot create worktree metadata in {}", directory.display()))?;
    serde_json::to_writer(
        &mut temporary,
        &OwnerRecord {
            version: OWNER_VERSION,
            owner_thread_id: thread_id.to_owned(),
        },
    )?;
    temporary.flush()?;

    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if owner(checkout)?.as_deref() == Some(thread_id) {
                Ok(())
            } else {
                bail!("worktree was concurrently assigned to another thread")
            }
        }
        Err(error) => Err(error.error)
            .with_context(|| format!("cannot write worktree owner at {}", path.display())),
    }
}

fn metadata_path(checkout: &Path) -> Result<PathBuf> {
    let path = git_path(checkout, ["rev-parse", "--git-path", OWNER_FILENAME])?;
    Ok(checkout.join(path))
}

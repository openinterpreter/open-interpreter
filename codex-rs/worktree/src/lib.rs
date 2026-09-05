mod git;
mod metadata;
mod settings;

use crate::git::git_path;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

pub use settings::DEFAULT_WORKTREE_KEEP_COUNT;
pub use settings::WorktreeSettings;

/// Creates and identifies worktrees using the existing Codex Desktop contract.
#[derive(Clone, Debug)]
pub struct WorktreeManager {
    settings: WorktreeSettings,
}

impl WorktreeManager {
    pub fn new(mut settings: WorktreeSettings) -> Self {
        settings.root = dunce::simplified(&settings.root).to_path_buf();
        Self { settings }
    }

    pub fn settings(&self) -> &WorktreeSettings {
        &self.settings
    }

    pub fn bind_thread(&self, checkout: &Path, thread_id: &str) -> Result<()> {
        let checkout = self.managed_checkout(checkout)?;
        metadata::bind_thread(&checkout, thread_id)
    }

    pub fn owner(&self, checkout: &Path) -> Result<Option<String>> {
        let checkout = self.managed_checkout(checkout)?;
        metadata::owner(&checkout)
    }

    fn managed_checkout(&self, checkout: &Path) -> Result<PathBuf> {
        let managed_root = dunce::canonicalize(&self.settings.root).with_context(|| {
            format!(
                "cannot resolve managed worktree root {}",
                self.settings.root.display()
            )
        })?;
        let checkout = dunce::canonicalize(checkout)
            .with_context(|| format!("cannot resolve worktree {}", checkout.display()))?;
        if !has_managed_layout(&managed_root, &checkout) {
            bail!("{} is not a managed worktree", checkout.display());
        }

        linked_worktree_common_dir(&checkout)?;
        Ok(checkout)
    }
}

fn linked_worktree_common_dir(checkout: &Path) -> Result<PathBuf> {
    if checkout != repository_root(checkout)? {
        bail!("{} is not a worktree root", checkout.display());
    }
    let git_dir = resolve_git_path(checkout, "--git-dir")?;
    let common_dir = resolve_git_path(checkout, "--git-common-dir")?;
    if git_dir == common_dir {
        bail!("{} is not a linked worktree", checkout.display());
    }
    Ok(common_dir)
}

fn has_managed_layout(root: &Path, checkout: &Path) -> bool {
    let Ok(relative) = checkout.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let Some(Component::Normal(bucket)) = components.next() else {
        return false;
    };
    let Some(bucket) = bucket.to_str() else {
        return false;
    };
    bucket.len() == 4
        && bucket.bytes().all(|byte| byte.is_ascii_hexdigit())
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
}

fn resolve_git_path(checkout: &Path, argument: &str) -> Result<PathBuf> {
    let path = checkout.join(git_path(checkout, ["rev-parse", argument])?);
    dunce::canonicalize(&path)
        .with_context(|| format!("cannot resolve Git path {}", path.display()))
}

fn repository_root(cwd: &Path) -> Result<PathBuf> {
    let path = git_path(cwd, ["rev-parse", "--show-toplevel"])?;
    dunce::canonicalize(&path)
        .with_context(|| format!("cannot resolve repository root {}", path.display()))
}

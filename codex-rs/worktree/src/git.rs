//! Executes Git metadata queries and decodes native filesystem paths.
//!
//! Commands remove inherited repository selectors and per-command Git config,
//! select the repository from the supplied working directory, and disable hooks
//! and filesystem monitoring.

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_git_utils::SAFE_BARE_REPOSITORY_CONFIG;
use codex_protocol::shell_environment::scrub_non_inheritable_env_vars;

const DISABLED_HOOKS_PATH: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

pub(crate) fn git_path<I, S>(cwd: &Path, args: I) -> Result<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = base_git_command(cwd)
        .args(args)
        .output()
        .context("failed to start git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git command failed: {}", stderr.trim());
    }
    let bytes = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
    #[cfg(windows)]
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    git_path_from_bytes(bytes)
}

pub(crate) fn git_path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    if bytes.is_empty() {
        bail!("git returned an empty path");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(
            std::str::from_utf8(bytes).context("git path is not valid UTF-8")?,
        ))
    }
}

fn base_git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    // Git wrappers export repository-local state to their children. Select this
    // repository from cwd, and install only our own per-command config below.
    for name in [
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_OBJECT_DIRECTORY",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_IMPLICIT_WORK_TREE",
        "GIT_GRAFT_FILE",
        "GIT_INDEX_FILE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_REPLACE_REF_BASE",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(name);
    }
    command
        .current_dir(cwd)
        .arg("-c")
        .arg(SAFE_BARE_REPOSITORY_CONFIG)
        .arg("-c")
        .arg(format!("core.hooksPath={DISABLED_HOOKS_PATH}"))
        .arg("-c")
        .arg("core.fsmonitor=")
        .arg("-c")
        .arg("attr.tree=")
        .arg("-c")
        .arg("core.attributesFile=")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    scrub_non_inheritable_env_vars(&mut command);
    command
}

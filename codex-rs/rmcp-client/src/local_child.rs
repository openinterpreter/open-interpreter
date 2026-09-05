//! Uniform child-process API for local MCP servers, with platform-specific spawning.
//!
//! Commands must use the launcher's cleared environment, process group, and default
//! argv[0]. Both implementations expose Tokio stdio handles and kill on drop.

use std::io;
use std::process::Stdio;

use tokio::process::Command;

#[cfg(target_os = "macos")]
#[path = "macos_stdio.rs"]
mod macos;

#[cfg(target_os = "macos")]
pub(super) use macos::LocalChild;
#[cfg(not(target_os = "macos"))]
pub(super) use tokio::process::Child as LocalChild;

pub(super) fn spawn(mut command: Command) -> io::Result<LocalChild> {
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "macos")]
    {
        LocalChild::spawn(command)
    }
    #[cfg(not(target_os = "macos"))]
    {
        command.spawn()
    }
}

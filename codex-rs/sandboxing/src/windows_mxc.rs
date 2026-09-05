//! Native Windows MXC capability discovery and availability metrics.
//! Probing is cached upstream; reporting happens at most once per process.

use appcontainer_common::base_container_runner::BaseContainerRunner;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

static AVAILABILITY_RECORDED: AtomicBool = AtomicBool::new(false);

// tracelogging 1.2.3 leaves its ETW import library to the application. These
// APIs are exported by Advapi32 in both the MSVC and GNU Windows toolchains.
#[link(name = "advapi32")]
unsafe extern "system" {}

pub(super) fn record_availability_once() {
    // Probe actual PSEC create/close support; symbol presence alone also
    // succeeds on transitional Windows builds where MXC is not enabled.
    let available = BaseContainerRunner::is_process_security_environment_usable();
    if let Some(metrics) = codex_otel::global()
        && !AVAILABILITY_RECORDED.swap(true, Ordering::Relaxed)
    {
        let _ = metrics.counter(
            "codex.windows_mxc.available",
            /*inc*/ 1,
            &[("available", if available { "true" } else { "false" })],
        );
    }
}

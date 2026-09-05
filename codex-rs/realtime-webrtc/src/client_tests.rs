//! Child environment admission excludes loader hooks, plugin search paths, and app credentials.

use pretty_assertions::assert_eq;

use super::child_environment;

#[test]
fn forwards_only_explicit_device_network_and_os_inputs() {
    let input = [
        ("SystemRoot", "C:\\Windows"),
        ("https_proxy", "http://proxy"),
        ("HOME", "/home/user"),
        ("LD_PRELOAD", "loader"),
        ("DYLD_INSERT_LIBRARIES", "loader"),
        ("PATH", "project"),
        ("GST_PLUGIN_PATH", "plugins"),
        ("OPENAI_API_KEY", "secret"),
    ];
    assert_eq!(
        child_environment(
            input
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
        ),
        input[..3]
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    );
}

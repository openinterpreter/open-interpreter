use super::accessible_entry;
use pretty_assertions::assert_eq;
use std::io;
use std::path::Path;

#[test]
fn unexpected_io_errors_abort_the_snapshot() {
    let error = io::Error::other("directory read interrupted");

    assert_eq!(
        accessible_entry::<()>(Err(error), Path::new("scan-root")),
        Err(
            "failed to enumerate unreadable glob paths under scan-root: directory read interrupted"
                .to_string()
        )
    );
}

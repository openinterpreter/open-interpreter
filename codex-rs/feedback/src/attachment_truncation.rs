use std::path::Path;

use anyhow::Context;
use anyhow::Result;

/// Shorten and label an attachment copy, leaving files within the target untouched.
/// JSONL keeps at least one complete record when a smaller record prefix is possible,
/// even if it exceeds the target. Callers must check the resulting upload size.
/// Other formats use a byte prefix.
pub(super) fn truncate_attachment(
    filename: &mut String,
    buffer: &mut Vec<u8>,
    target_bytes: usize,
) -> Result<()> {
    if buffer.len() <= target_bytes {
        return Ok(());
    }

    let end = match Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("jsonl") => buffer[..target_bytes]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .or_else(|| {
                buffer
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .filter(|end| *end + 1 < buffer.len())
            })
            .map(|end| end + 1)
            .context("feedback attachment cannot be shortened to a complete JSONL record")?,
        _ => target_bytes,
    };
    buffer.truncate(end);
    if !filename.starts_with("truncated-") {
        filename.insert_str(0, "truncated-");
    }
    Ok(())
}

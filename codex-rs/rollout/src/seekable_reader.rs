//! Reads rollout byte positions independently of their plain or compressed representation.
//!
//! Offsets always address the original JSONL bytes. Readers retain an open file, or an anonymous
//! decoded snapshot, so a concurrent compression cannot invalidate an in-progress scan.

use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::time::Duration;

use crate::plain_rollout_path;

enum RolloutReader {
    Plain(File),
    Compressed(File),
}

impl RolloutReader {
    fn open(path: &Path) -> io::Result<Self> {
        let plain_path = plain_rollout_path(path);
        let compressed_path = plain_path.with_extension("jsonl.zst");
        for attempt in 0..4 {
            match File::open(&plain_path) {
                Ok(file) => return Ok(Self::Plain(file)),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
            match File::open(&compressed_path) {
                Ok(file) => return Ok(Self::Compressed(file)),
                Err(err) if err.kind() == io::ErrorKind::NotFound && attempt < 3 => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("the final open attempt returns its error")
    }
}

/// Opens the original JSONL bytes for blocking offset reads without changing the rollout on disk.
///
/// Compressed files are decoded into an anonymous temporary file, keeping memory use bounded and
/// preserving read-only access to the Codex home. Plain files retain their existing seek fast path.
pub fn open_rollout_seekable_reader(path: &Path) -> io::Result<File> {
    match RolloutReader::open(path)? {
        RolloutReader::Plain(file) => Ok(file),
        RolloutReader::Compressed(file) => {
            let mut decoded = tempfile::tempfile()?;
            io::copy(&mut zstd::stream::read::Decoder::new(file)?, &mut decoded)?;
            decoded.rewind()?;
            Ok(decoded)
        }
    }
}

/// Checks a frozen prefix's byte bound using a blocking read of the logical JSONL representation.
///
/// A known first-frame size is a lower bound even for concatenated zstd frames. New compressed
/// rollouts include that size, so ordinary lineage validation only reads the header. Older frames
/// without a size, and prefixes extending beyond the first frame, are decoded up to the bound.
pub fn rollout_contains_prefix(path: &Path, end_byte_offset: u64) -> io::Result<bool> {
    match RolloutReader::open(path)? {
        RolloutReader::Plain(file) => Ok(end_byte_offset <= file.metadata()?.len()),
        RolloutReader::Compressed(mut file) => {
            // A zstd frame header occupies at most 18 bytes.
            let mut header = [0; 18];
            let read = file.read(&mut header)?;
            if zstd::zstd_safe::get_frame_content_size(&header[..read])
                .ok()
                .flatten()
                .is_some_and(|size| end_byte_offset <= size)
            {
                return Ok(true);
            }
            file.rewind()?;
            let mut prefix = zstd::stream::read::Decoder::new(file)?.take(end_byte_offset);
            Ok(io::copy(&mut prefix, &mut io::sink())? == end_byte_offset)
        }
    }
}

#[cfg(test)]
#[path = "seekable_reader_tests.rs"]
mod tests;

//! Answer a bounded set of blocking terminal queries without emulating a terminal.

use codex_utils_pty::SpawnedProcess;
use tokio::sync::mpsc;

// Each pair contains the query bytes and the response to write to stdin.
const QUERY_RESPONSES: [(&[u8], &[u8]); 3] = [
    // Device status report: report that the terminal is operating normally.
    (b"\x1b[5n", b"\x1b[0n"),
    // Window-size query: report a 24-row, 80-column text area.
    (b"\x1b[18t", b"\x1b[8;24;80t"),
    // Cursor-position report: return row 1, column 1.
    (b"\x1b[6n", b"\x1b[1;1R"),
];
const MAX_MODE_DIGITS: usize = 10;
const MAX_QUERY_BYTES: usize = MAX_MODE_DIGITS + 5;

#[derive(Default)]
struct TerminalQueryResponder {
    pending: Vec<u8>,
}

impl TerminalQueryResponder {
    fn process(&mut self, bytes: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
        if self.pending.is_empty() && !bytes.contains(&b'\x1b') {
            return (bytes, Vec::new());
        }

        let mut output = Vec::with_capacity(bytes.len());
        let mut responses = Vec::new();

        for byte in bytes {
            if self.pending.is_empty() && byte != b'\x1b' {
                output.push(byte);
                continue;
            }

            if byte == b'\x1b' {
                output.append(&mut self.pending);
            }
            self.pending.push(byte);
            if self.pending.len() == 1
                || self.pending.as_slice() == b"\x1b["
                || self.pending[1] == b'['
                    && !(0x40..=0x7e).contains(&byte)
                    && self.pending.len() < MAX_QUERY_BYTES
            {
                continue;
            }

            if let Some((_, response)) = QUERY_RESPONSES
                .iter()
                .find(|(query, _)| *query == self.pending.as_slice())
            {
                responses.extend_from_slice(response);
            } else if let [b'\x1b', b'[', b'?', mode @ .., b'$', b'p'] = self.pending.as_slice()
                && !mode.is_empty()
                && mode.len() <= MAX_MODE_DIGITS
                && mode.iter().all(u8::is_ascii_digit)
            {
                // DEC private-mode query: report the requested mode as unrecognized.
                responses.extend_from_slice(b"\x1b[?");
                responses.extend_from_slice(mode);
                responses.extend_from_slice(b";0$y");
            } else {
                output.append(&mut self.pending);
            }
            self.pending.clear();
        }

        (output, responses)
    }
}

pub(crate) fn respond_to_terminal_queries(mut spawned: SpawnedProcess) -> SpawnedProcess {
    let response_tx = spawned.session.writer_sender().downgrade();
    let (output_tx, output_rx) = mpsc::channel(128);
    let mut raw_output_rx = std::mem::replace(&mut spawned.stdout_rx, output_rx);

    tokio::spawn(async move {
        let mut responder = TerminalQueryResponder::default();
        while let Some(bytes) = raw_output_rx.recv().await {
            let (output, responses) = responder.process(bytes);
            if !responses.is_empty()
                && let Some(writer) = response_tx.upgrade()
            {
                let _ = writer.send(responses).await;
            }
            if !output.is_empty() {
                let _ = output_tx.send(output).await;
            }
        }

        if !responder.pending.is_empty() {
            let _ = output_tx.send(responder.pending).await;
        }
    });

    spawned
}

#[cfg(test)]
#[path = "terminal_queries_tests.rs"]
mod tests;

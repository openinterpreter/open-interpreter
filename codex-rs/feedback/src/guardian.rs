//! Bounded process-local failed-review evidence, shared by root and child reviewers.
//! Captures are exported only by a feedback request that includes logs.

use crate::FeedbackAttachment;
use codex_protocol::ThreadId;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Mutex;

const MAX_RECORDS: usize = 64;
const MAX_RECORDS_PER_THREAD: usize = 8;
const MAX_BYTES: usize = 8 * 1024 * 1024;
static RECORDS: Mutex<ReviewRecords> = Mutex::new(ReviewRecords {
    records: VecDeque::new(),
    bytes: 0,
    discarded_records: 0,
});

/// A consistent snapshot of retained failures and their owning threads.
pub struct GuardianReviewFailures {
    pub attachment: Option<FeedbackAttachment>,
    /// Unique threads, ordered by most recently retained failure first.
    pub thread_ids: Vec<ThreadId>,
    /// Process-wide evictions or oversized records, not a count for this task tree.
    pub process_discarded_records: usize,
}

#[derive(Default)]
struct ReviewRecords {
    records: VecDeque<(ThreadId, Vec<u8>)>,
    bytes: usize,
    discarded_records: usize,
}

impl ReviewRecords {
    fn push(&mut self, thread_id: ThreadId, record: Vec<u8>) {
        if record.len() + 1 > MAX_BYTES {
            self.discarded_records = self.discarded_records.saturating_add(1);
            return;
        }
        if self
            .records
            .iter()
            .filter(|(id, _)| *id == thread_id)
            .count()
            >= MAX_RECORDS_PER_THREAD
            && let Some(index) = self.records.iter().position(|(id, _)| *id == thread_id)
            && let Some((_, removed)) = self.records.remove(index)
        {
            self.bytes -= removed.len() + 1;
            self.discarded_records = self.discarded_records.saturating_add(1);
        }
        while self.records.len() >= MAX_RECORDS || self.bytes + record.len() + 1 > MAX_BYTES {
            if let Some((_, removed)) = self.records.pop_front() {
                self.bytes -= removed.len() + 1;
                self.discarded_records = self.discarded_records.saturating_add(1);
            }
        }
        self.bytes += record.len() + 1;
        self.records.push_back((thread_id, record));
    }

    fn snapshot(&self, thread_ids: &[ThreadId]) -> GuardianReviewFailures {
        let thread_ids = thread_ids.iter().copied().collect::<HashSet<_>>();
        let mut buffer = Vec::new();
        for (thread_id, record) in &self.records {
            if thread_ids.contains(thread_id) {
                buffer.extend_from_slice(record);
                buffer.push(b'\n');
            }
        }
        let attachment = (!buffer.is_empty()).then_some(FeedbackAttachment {
            filename: "auto-review-failures.jsonl".to_string(),
            buffer,
            content_type: Some("application/x-ndjson".to_string()),
        });
        let mut seen = HashSet::new();
        let thread_ids = self
            .records
            .iter()
            .rev()
            .filter(|(id, _)| thread_ids.contains(id) && seen.insert(*id))
            .map(|(id, _)| *id)
            .collect();
        GuardianReviewFailures {
            attachment,
            thread_ids,
            process_discarded_records: self.discarded_records,
        }
    }
}

/// Retain one complete, serialized failed-review JSON record for later opt-in feedback.
pub fn record_guardian_review_failure(thread_id: ThreadId, record: Vec<u8>) {
    RECORDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(thread_id, record);
}

/// Snapshot failures for the reported task tree without including other tasks' records.
pub fn guardian_review_failures(thread_ids: &[ThreadId]) -> GuardianReviewFailures {
    RECORDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot(thread_ids)
}

#[cfg(test)]
#[path = "guardian_tests.rs"]
mod tests;

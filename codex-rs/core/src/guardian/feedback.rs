//! Captures bounded failed-review evidence for opt-in feedback uploads.
//! Records outlive replaced and short-lived reviewers without writing extra history files.

use super::GuardianAssessmentOutcome;
use super::approval_request::format_guardian_action_pretty;
use super::approval_request::guardian_request_target_item_id;
use super::approval_request::guardian_request_turn_id;
use super::prompt::parse_guardian_assessment;
use super::review_session::GuardianReviewSessionOutcome;
use super::review_session::GuardianReviewSessionParams;
use crate::session::session::Session;
use codex_feedback::record_guardian_review_failure;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseItem;
use serde::Serialize;
use std::io;
use std::io::Write;

const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize)]
struct ReviewFeedbackRecord<'a> {
    reviewed_thread_id: ThreadId,
    reviewed_turn_id: &'a str,
    target_item_id: Option<&'a str>,
    reviewer_thread_id: ThreadId,
    model: &'a str,
    status: &'a str,
    decision: Option<&'a str>,
    action: &'a str,
    action_truncated: bool,
    instructions: Option<&'a str>,
    history: Vec<&'a ResponseItem>,
    context_omitted: bool,
}

pub(super) async fn record_failed_review(
    reviewer: &Session,
    params: &GuardianReviewSessionParams,
    outcome: &GuardianReviewSessionOutcome,
) {
    if !params.spawn_config.feedback_enabled || params.spawn_config.ephemeral {
        return;
    }
    let (status, decision) = match outcome {
        GuardianReviewSessionOutcome::Completed(Ok(Some(decision))) => {
            match parse_guardian_assessment(Some(decision)) {
                Ok(assessment) if assessment.outcome == GuardianAssessmentOutcome::Allow => {
                    return;
                }
                Ok(_) => ("denied", Some(decision.as_str())),
                Err(_) => ("invalid_decision", Some(decision.as_str())),
            }
        }
        GuardianReviewSessionOutcome::Completed(Ok(None)) => ("missing_decision", None),
        GuardianReviewSessionOutcome::Completed(Err(_))
        | GuardianReviewSessionOutcome::PromptBuildFailed(_)
        | GuardianReviewSessionOutcome::SessionFailed { .. } => ("failed", None),
        GuardianReviewSessionOutcome::TimedOut => ("timed_out", None),
        GuardianReviewSessionOutcome::Aborted => ("aborted", None),
    };
    let Ok(action) = format_guardian_action_pretty(&params.request) else {
        tracing::warn!("Could not serialize Guardian feedback action");
        return;
    };
    let instructions = reviewer.get_prompt_base_instructions().await;
    let history = reviewer.clone_history().await;
    ReviewFeedbackRecord {
        reviewed_thread_id: params.parent_session.thread_id(),
        reviewed_turn_id: guardian_request_turn_id(
            &params.request,
            &params.parent_context.turn().sub_id,
        ),
        target_item_id: guardian_request_target_item_id(&params.request),
        reviewer_thread_id: reviewer.thread_id(),
        model: &params.model,
        status,
        decision,
        action: &action.text,
        action_truncated: action.truncated,
        instructions: Some(&instructions.text),
        history: history.raw_items().collect(),
        context_omitted: false,
    }
    .store();
}

impl ReviewFeedbackRecord<'_> {
    fn store(mut self) {
        let mut buffer = BoundedBuffer(Vec::new());
        if serde_json::to_writer(&mut buffer, &self).is_err() {
            // Preserve the action and decision even when optional reviewer history is too large.
            self.instructions = None;
            self.history.clear();
            self.context_omitted = true;
            buffer = BoundedBuffer(Vec::new());
            if serde_json::to_writer(&mut buffer, &self).is_err() {
                tracing::warn!("Guardian feedback action and decision exceed the record limit");
                return;
            }
        }
        record_guardian_review_failure(self.reviewed_thread_id, buffer.0);
    }
}

struct BoundedBuffer(Vec<u8>);

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RECORD_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other(
                "Guardian feedback record exceeds its size limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "feedback_tests.rs"]
mod tests;

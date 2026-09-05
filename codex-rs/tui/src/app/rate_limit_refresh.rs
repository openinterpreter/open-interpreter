//! Orders full usage reads and coalesces recovery refreshes within an account/limit generation.

use crate::app_event::RateLimitRefreshOrigin;

#[derive(Default)]
pub(super) struct RateLimitRefreshState {
    next_id: u64,
    applied_id: u64,
    recovery: Option<PendingRecovery>,
}

struct PendingRecovery {
    request: (u64, u64),
    origin: RateLimitRefreshOrigin,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RateLimitRefreshOutcome {
    Apply,
    Ignore,
    RefreshRecovery,
}

#[derive(PartialEq, Eq)]
pub(super) enum RateLimitReadStatus {
    Succeeded,
    Failed,
}

impl RateLimitRefreshState {
    pub(super) fn has_pending_recovery(&self) -> bool {
        self.recovery.is_some()
    }

    pub(super) fn start(
        &mut self,
        origin: RateLimitRefreshOrigin,
        generation: &mut u64,
    ) -> Option<(u64, u64)> {
        if matches!(origin, RateLimitRefreshOrigin::Recovery) {
            // A new hard error supersedes a post-reset read that started before the error.
            if self
                .recovery
                .as_ref()
                .is_some_and(|pending| pending.origin == RateLimitRefreshOrigin::Recovery)
            {
                return None;
            }
            // A hard error invalidates even an older read without a preceding rolling notification.
            *generation = generation.wrapping_add(1);
        }
        self.next_id = self.next_id.wrapping_add(1);
        let request = (self.next_id, *generation);
        // The post-reset read takes over the hold from the invalidated recovery request.
        if matches!(
            origin,
            RateLimitRefreshOrigin::Recovery | RateLimitRefreshOrigin::ResetConsume { .. }
        ) {
            self.recovery = Some(PendingRecovery { request, origin });
        }
        Some(request)
    }

    // Account changes and successful resets invalidate the need as well as the old response.
    pub(super) fn invalidate_recovery(&mut self) {
        self.recovery = None;
    }

    pub(super) fn finish(
        &mut self,
        request_id: u64,
        generation: u64,
        current_generation: u64,
        status: RateLimitReadStatus,
    ) -> RateLimitRefreshOutcome {
        if self
            .recovery
            .as_ref()
            .is_some_and(|pending| pending.request == (request_id, generation))
        {
            self.recovery = None;
            // A rolling hard stop can arrive after the error, without another error notification.
            if generation != current_generation {
                return RateLimitRefreshOutcome::RefreshRecovery;
            }
        }
        if generation != current_generation
            || request_id < self.applied_id
            || status == RateLimitReadStatus::Failed
        {
            return RateLimitRefreshOutcome::Ignore;
        }
        self.applied_id = request_id;
        RateLimitRefreshOutcome::Apply
    }
}

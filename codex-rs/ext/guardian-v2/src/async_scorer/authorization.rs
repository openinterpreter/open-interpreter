//! Binds cached classifier results to the user authorization they evaluated.

use codex_core::CodexThread;
use codex_core::GuardianAuthorizationVersion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ScoreAuthorization {
    pub(super) local: GuardianAuthorizationVersion,
    pub(super) root: Option<GuardianAuthorizationVersion>,
}

impl ScoreAuthorization {
    pub(super) async fn current(thread: &CodexThread) -> Self {
        let root = thread
            .guardian_root_snapshot()
            .await
            .map(|snapshot| snapshot.authorization_version);
        Self {
            local: thread.guardian_authorization_version().await,
            root,
        }
    }
}

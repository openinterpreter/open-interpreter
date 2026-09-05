//! Model-invisible checkpoint of the host's bounded Guardian transcript.

use codex_protocol::models::ResponseItem;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Original review evidence, separate from the compacted model conversation.
/// Hosts enforce transcript retention limits both when saving and restoring it.
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GuardianHistoryCheckpoint(pub Vec<ResponseItem>);

impl std::fmt::Debug for GuardianHistoryCheckpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GuardianHistoryCheckpoint")
            .field("items", &self.0.len())
            .finish()
    }
}

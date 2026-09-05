use codex_protocol::models::ResponseItem;

/// Read-only conversation-history snapshot supplied by the extension host.
///
/// Implementations should retain the host's existing snapshot storage rather than
/// copying response payloads into an extension-owned collection.
pub trait ConversationHistorySnapshot: Send + Sync {
    /// Returns the generation of the history captured by this snapshot.
    fn history_version(&self) -> u64;

    /// Host-owned revision captured with this snapshot. Advances on user messages and
    /// history resets, but stays unchanged for compaction and internal context.
    fn user_message_revision(&self) -> u64;

    /// Returns the snapshot's response items in conversation order.
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_>;

    /// Original review evidence retained across parent compaction, in conversation order.
    /// Hosts without separate retention provide their current history.
    fn review_items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_> {
        self.items()
    }

    /// Changes whenever offsets into the retained review evidence become invalid.
    fn review_history_version(&self) -> u64 {
        self.history_version()
    }
}

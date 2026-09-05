//! Shared user-message selection, independent of rendering and non-user eviction.
//!
//! Callers supply costs including their own formatting. The first user message
//! remains an anchor even when it exceeds the budget, preserving existing behavior.

/// Rendered cost and original transcript position of a genuine user message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMessageCost {
    pub index: usize,
    pub tokens: usize,
}

/// Selected original indices and their total rendered token cost.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct UserMessageSelection {
    pub indices: Vec<usize>,
    pub tokens: usize,
}

/// Keeps the first and latest user messages, then fills from newest to oldest.
///
/// Supply messages in conversation order with distinct original indices. The
/// returned indices are in selection order; callers render in conversation order.
/// Non-user evidence uses the remaining budget under the caller's own policy.
pub fn select_user_messages(
    messages: &[UserMessageCost],
    max_message_tokens: usize,
) -> UserMessageSelection {
    let Some((first, remaining)) = messages.split_first() else {
        return UserMessageSelection::default();
    };
    let mut selection = UserMessageSelection {
        indices: vec![first.index],
        tokens: first.tokens,
    };
    // The latest message is considered first, including when it cannot fit.
    for message in remaining.iter().rev() {
        if selection.tokens.saturating_add(message.tokens) <= max_message_tokens {
            selection.indices.push(message.index);
            selection.tokens += message.tokens;
        }
    }
    selection
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;

//! Offline editing bypasses command dispatch and popup actions, retaining the draft in place.

use super::*;

impl ChatComposer {
    pub(crate) fn handle_disconnected_key(&mut self, key: KeyEvent) {
        self.cancel_history_search();
        self.attachments.clear_remote_image_selection();
        self.popups.active = ActivePopup::None;
        self.set_disable_paste_burst(/*disabled*/ true);
        // Expand in reverse order so remaining ranges stay valid. Textarea replacement
        // preserves the cursor and other elements, including images and mentions.
        let pending_pastes = std::mem::take(&mut self.draft.pending_pastes);
        if !pending_pastes.is_empty() {
            for element in self.draft.textarea.text_elements().into_iter().rev() {
                if let Some((_, text)) = pending_pastes.iter().find(|(placeholder, _)| {
                    element.placeholder(self.draft.textarea.text()) == Some(placeholder.as_str())
                }) {
                    self.draft
                        .textarea
                        .replace_range(element.byte_range.start..element.byte_range.end, text);
                }
            }
        }
        // Enter/Tab and configured submit bindings must never consume the draft offline.
        // The basic editor reconciles attachments without invoking composer-level shortcuts.
        if !matches!(key.code, KeyCode::Enter | KeyCode::Tab)
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            self.handle_input_basic(key);
        }
    }
}

#[cfg(test)]
#[path = "reconnect_tests.rs"]
mod tests;

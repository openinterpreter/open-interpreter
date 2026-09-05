//! Offline edits retain the same attachment bookkeeping as ordinary text edits.

use super::super::tests::new_test_composer;
use super::*;
use pretty_assertions::assert_eq;

#[test]
fn reconnect_expands_pastes_preserving_images_and_cursor() {
    let (mut composer, _) = new_test_composer();
    let paste = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1);
    composer.handle_paste(paste.clone());
    composer.attach_image(PathBuf::from("local.png"));
    composer.handle_paste(paste);
    let expanded = composer.current_text_with_pending();
    let images = composer.draft_snapshot().local_images;
    composer.handle_disconnected_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    let draft = composer.draft_snapshot();
    assert_eq!(
        (
            draft.text,
            draft.cursor,
            draft.local_images,
            draft.pending_pastes
        ),
        (
            format!("{expanded}!"),
            expanded.len() + 1,
            images,
            Vec::new()
        )
    );
    assert_eq!(draft.text_elements.len(), 1);
}

#[test]
fn reconnect_edit_removes_deleted_attachment_and_paste_metadata() {
    let (mut composer, _) = new_test_composer();
    composer.attach_image(PathBuf::from("local.png"));
    composer.handle_paste("x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1));
    composer.handle_disconnected_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    let draft = composer.draft_snapshot();
    assert_eq!(
        (draft.text, draft.local_images, draft.pending_pastes),
        (String::new(), Vec::new(), Vec::new())
    );
}

#[test]
fn reconnect_edit_cancels_history_preview_without_losing_original_draft() {
    let (mut composer, _) = new_test_composer();
    composer.set_text_content("original draft".into(), Vec::new(), Vec::new());
    composer.draft.textarea.set_cursor(/*pos*/ 14);
    composer.begin_history_search();
    composer.apply_history_search_result(HistorySearchResult::Found(HistoryEntry::new(
        "history preview".into(),
    )));
    composer.handle_disconnected_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
    assert_eq!(composer.current_text(), "original draft!");
    assert!(!composer.history_search_active());
}

use codex_protocol::models::ReasoningItemContent;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn message(text: &str) -> ResponseItem {
    serde_json::from_value(json!({
        "type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]
    }))
    .unwrap()
}

fn tool(text: &str) -> ResponseItem {
    serde_json::from_value(json!({
        "type": "function_call_output", "call_id": "call", "output": text
    }))
    .unwrap()
}

#[test]
fn rollback_keeps_the_earlier_prefix_or_clears_an_evicted_boundary() {
    let items = [message("keep"), message("roll back"), tool("later")];
    let mut history = TranscriptHistory::default();
    history.reset(items.iter());
    let generation = history.generation();
    history.truncate_before(&items[1]);
    assert_eq!(history.items().collect::<Vec<_>>(), vec![&items[0]]);
    assert!(history.generation() > generation);
    history.truncate_before(&items[1]);
    assert_eq!(history.items().count(), 0);
}

#[test]
fn each_kind_evicts_its_own_oldest_entries_without_reordering() {
    let users = [message("first"), message("second"), message("third")];
    let tools: Vec<_> = (0..MAX_ITEMS_PER_KIND)
        .map(|index| tool(&index.to_string()))
        .collect();
    let mut history = TranscriptHistory::default();
    history.record(&users[0]);
    history.record(&tool("old output"));
    history.record(&users[1]);
    for item in &tools {
        history.record(item);
    }
    history.record(&users[2]);
    assert_eq!(
        history.items().collect::<Vec<_>>(),
        users[..2]
            .iter()
            .chain(&tools)
            .chain(&users[2..])
            .collect::<Vec<_>>()
    );
    let generation = history.generation();
    let newer_users: Vec<_> = (0..MAX_ITEMS_PER_KIND)
        .map(|index| message(&index.to_string()))
        .collect();
    for item in &newer_users {
        history.record(item);
    }
    assert_eq!(
        history.items().collect::<Vec<_>>(),
        tools.iter().chain(&newer_users).collect::<Vec<_>>()
    );
    assert!(history.generation() > generation);
}

#[test]
fn byte_limits_are_independent_and_oversized_items_do_not_clear_history() {
    let large = tool(&"x".repeat(MAX_BYTES_PER_KIND / 2));
    let keep = message("keep this");
    let mut history = TranscriptHistory::default();
    history.record(&large);
    history.record(&keep);
    history.record(&large);
    assert_eq!(history.items().collect::<Vec<_>>(), vec![&keep, &large]);

    let oversized = "x".repeat(MAX_BYTES_PER_KIND);
    history.record(&message(&oversized));
    history.record(&tool(&oversized));
    history.record(&ResponseItem::Reasoning {
        id: None,
        summary: Vec::new(),
        // Serialization omits this content, but retention must still count it.
        content: Some(vec![ReasoningItemContent::Text { text: oversized }]),
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });
    assert_eq!(history.items().collect::<Vec<_>>(), vec![&keep, &large]);
}

#[test]
fn oversized_user_images_preserve_text_and_metadata_in_order() {
    let before = tool("earlier tool result");
    let after = message("later message");
    let text_only: ResponseItem = serde_json::from_value(json!({
        "type": "message", "id": "user-1", "role": "user", "phase": "commentary",
        "internal_chat_message_metadata_passthrough": {"turn_id": "turn-1"},
        "content": [
            {"type": "input_text", "text": "Do not publish anything."},
            {"type": "output_text", "text": "Only inspect the attached image."}
        ]
    }))
    .unwrap();
    for image_bytes in [16, MAX_BYTES_PER_KIND] {
        let mut with_image = text_only.clone();
        let ResponseItem::Message { content, .. } = &mut with_image else {
            unreachable!()
        };
        content.insert(
            /*index*/ 1,
            ContentItem::InputImage {
                image_url: format!("data:image/png;base64,{}", "A".repeat(image_bytes)),
                detail: Some(codex_protocol::models::ImageDetail::Original),
            },
        );
        let mut history = TranscriptHistory::default();
        history.record(&before);
        history.record(&with_image);
        history.record(&after);
        let expected = if image_bytes < MAX_BYTES_PER_KIND {
            &with_image
        } else {
            &text_only
        };
        assert_eq!(
            history.items().collect::<Vec<_>>(),
            vec![&before, expected, &after]
        );
    }
}

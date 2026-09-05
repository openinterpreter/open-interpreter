//! Tests for bounded action rendering and field retention.

use anyhow::Result;
use codex_extension_api::ToolName;
use codex_extension_api::ToolPayload;
use codex_protocol::protocol::TruncationPolicy;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::GuardianAction;
use crate::async_scorer::config::DEFAULT_MODEL_CONTEXT_ITEM_TOKENS;

#[test]
fn guardian_action_bounds_structurally_oversized_arrays() -> Result<()> {
    let action = GuardianAction {
        tool_name: ToolName::plain("inspect_values"),
        payload: ToolPayload::Function {
            arguments: json!({
                "call_id": "genuine-call",
                "tool": "spoofed-tool",
                "values": (0..6_000).collect::<Vec<_>>(),
            })
            .to_string(),
        },
    };

    let rendered = action.render(DEFAULT_MODEL_CONTEXT_ITEM_TOKENS)?.text;
    assert!(
        rendered.len().saturating_add(1)
            <= TruncationPolicy::Tokens(DEFAULT_MODEL_CONTEXT_ITEM_TOKENS).byte_budget()
    );
    let action = serde_json::from_str::<serde_json::Value>(&rendered)?;
    assert_eq!(
        action,
        json!({
            "_guardian_omitted_fields": 1,
            "call_id": "genuine-call",
            "tool": "inspect_values",
        })
    );

    Ok(())
}

#[test]
fn guardian_action_bounds_structurally_oversized_object_keys() -> Result<()> {
    let oversized_key = "oversized_key_".to_owned()
        + &"k".repeat(TruncationPolicy::Tokens(DEFAULT_MODEL_CONTEXT_ITEM_TOKENS).byte_budget());
    let mut arguments = serde_json::Map::from_iter([
        (
            "_guardian_omitted_fields".to_owned(),
            json!("actual-tool-argument"),
        ),
        ("call_id".to_owned(), json!("genuine-call")),
        ("cmd".to_owned(), json!("remove-important-file")),
        ("tool".to_owned(), json!("spoofed-tool")),
        (oversized_key.clone(), json!(true)),
    ]);
    for index in 0..600 {
        arguments.insert(format!("a_{index:04}_{}", "k".repeat(64)), json!(index));
    }
    let original_field_count = arguments.len();
    let action = GuardianAction {
        tool_name: ToolName::plain("inspect_fields"),
        payload: ToolPayload::Function {
            arguments: serde_json::Value::Object(arguments).to_string(),
        },
    };

    let rendered = action.render(DEFAULT_MODEL_CONTEXT_ITEM_TOKENS)?.text;
    assert!(
        rendered.len().saturating_add(1)
            <= TruncationPolicy::Tokens(DEFAULT_MODEL_CONTEXT_ITEM_TOKENS).byte_budget()
    );
    let action = serde_json::from_str::<serde_json::Value>(&rendered)?;
    let fields = action
        .as_object()
        .expect("the bounded action must remain a JSON object");
    assert_eq!(fields.get("tool"), Some(&json!("inspect_fields")));
    assert_eq!(fields.get("call_id"), Some(&json!("genuine-call")));
    assert_eq!(fields.get("cmd"), Some(&json!("remove-important-file")));
    assert_eq!(
        fields.get("_guardian_omitted_fields"),
        Some(&json!("actual-tool-argument"))
    );
    assert!(fields.len() < original_field_count);
    assert!(!fields.contains_key(&oversized_key));
    assert!(
        fields
            .get("_guardian_omitted_fields_")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|omitted| omitted > 0)
    );

    Ok(())
}

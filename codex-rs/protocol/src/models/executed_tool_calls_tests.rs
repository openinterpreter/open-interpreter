use anyhow::Result;
use pretty_assertions::assert_eq;

use super::super::FunctionCallOutputBody;
use super::super::FunctionCallOutputPayload;
use super::super::ResponseInputItem;
use super::*;

fn passthrough_metadata(turn_id: &str) -> InternalChatMessageMetadataPassthrough {
    InternalChatMessageMetadataPassthrough {
        turn_id: Some(turn_id.to_string()),
        ..Default::default()
    }
}

#[test]
fn executed_tool_call_prompt_budget_includes_metadata_fields() -> Result<()> {
    let metadata_bytes = |items: &[ResponseItem]| -> Result<usize> {
        items.iter().try_fold(0_usize, |bytes, item| {
            let with_metadata = serde_json::to_vec(item)?.len();
            let mut without_metadata = item.clone();
            without_metadata.clear_executed_tool_calls();
            let without_metadata = serde_json::to_vec(&without_metadata)?.len();
            Ok(bytes + with_metadata.saturating_sub(without_metadata))
        })
    };

    for with_turn_id in [true, false] {
        let mut items = (0..1_000)
            .map(|index| {
                let mut item = ResponseItem::FunctionCallOutput {
                    id: None,
                    call_id: Some(index.to_string()),
                    name: None,
                    namespace: None,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(String::new()),
                        success: None,
                    },
                    internal_chat_message_metadata_passthrough: with_turn_id
                        .then(|| passthrough_metadata("turn-1")),
                };
                item.append_executed_tool_calls(vec![ExecutedToolCall::new(
                    String::new(),
                    serde_json::Value::Null,
                )]);
                item.set_tool_call_cell_id("cell-\"\\");
                item.mark_tool_calls_complete();
                item
            })
            .collect::<Vec<_>>();

        assert!(metadata_bytes(&items)? > MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
        bound_executed_tool_calls_for_prompt(&mut items);
        assert!(metadata_bytes(&items)? <= MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
        for metadata in items
            .iter()
            .filter_map(ResponseItem::executed_tool_call_metadata)
        {
            assert_eq!(metadata.tool_calls_complete, None);
        }

        let calls = items
            .iter()
            .filter_map(ResponseItem::executed_tool_call_metadata)
            .filter_map(|metadata| metadata.executed_tool_calls.as_ref())
            .flatten()
            .collect::<Vec<_>>();
        let omitted_calls = calls
            .iter()
            .filter_map(|call| call.truncation())
            .filter_map(|truncation| truncation.omitted_calls)
            .sum::<usize>();
        assert!(omitted_calls > 0);
        assert_eq!(calls.len() + omitted_calls, 1_000);
        assert!(calls.iter().any(|call| {
            matches!(
                call.truncation(),
                Some(truncation) if truncation.omitted_calls == Some(omitted_calls)
            )
        }));

        let bounded_items = items.clone();
        bound_executed_tool_calls_for_prompt(&mut items);
        assert_eq!(items, bounded_items);

        let oversized_name = "\0".repeat(MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
        let mut oversized_items = (0..2)
            .map(|index| {
                let mut item = ResponseItem::FunctionCallOutput {
                    id: None,
                    call_id: Some(index.to_string()),
                    name: None,
                    namespace: None,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(String::new()),
                        success: None,
                    },
                    internal_chat_message_metadata_passthrough: with_turn_id
                        .then(|| passthrough_metadata("turn-1")),
                };
                item.append_executed_tool_calls(vec![ExecutedToolCall::new(
                    oversized_name.clone(),
                    serde_json::Value::Null,
                )]);
                item.set_tool_call_cell_id("cell-\"\\");
                item
            })
            .collect::<Vec<_>>();

        bound_executed_tool_calls_for_prompt(&mut oversized_items);
        assert!(metadata_bytes(&oversized_items)? <= MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
        let calls = oversized_items
            .iter()
            .filter_map(ResponseItem::executed_tool_call_metadata)
            .filter_map(|metadata| metadata.executed_tool_calls.as_ref())
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let call = calls[0];
        assert_eq!(
            oversized_items
                .iter()
                .filter_map(ResponseItem::executed_tool_call_metadata)
                .find_map(|metadata| metadata.cell_id.as_deref()),
            Some("cell-\"\\"),
        );
        assert_eq!(call.name.len(), MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES / 2);
        assert!(oversized_name.starts_with(&call.name));
        let truncation = call.truncation().expect("trusted omission marker");
        assert_eq!(truncation.omitted_calls, Some(1));
        assert_eq!(truncation.original_name_bytes, Some(oversized_name.len()));
        assert_eq!(
            serde_json::to_value(&call.arguments)?["_codex_executed_tool_call_truncated"]["original_name_bytes"],
            serde_json::json!(oversized_name.len()),
        );

        let bounded_items = oversized_items.clone();
        bound_executed_tool_calls_for_prompt(&mut oversized_items);
        assert_eq!(oversized_items, bounded_items);
    }

    let mut items = ["exec", "wait"].map(|call_id| {
        let mut item = ResponseItem::from(ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output: FunctionCallOutputPayload::from_text(String::new()),
        });
        item.set_tool_call_cell_id("cell-\"\\");
        item
    });
    items[0].append_executed_tool_calls(vec![
        ExecutedToolCall::new(
            "test_tool".to_string(),
            serde_json::json!({ "payload": "x".repeat(MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES - 256) }),
        );
        4
    ]);
    items[1].mark_tool_calls_complete();
    let expected = items.clone();
    assert!(
        first_executed_tool_call(&mut items[0])
            .expect("recorded call should exist")
            .set_tool_result_sources(ToolResultSources::new(
                (0..MAX_TOOL_RESULT_SOURCES)
                    .map(|index| ToolResultSource {
                        r#type: "document".to_string(),
                        id: format!(
                            "R{index:0width$}",
                            width = MAX_TOOL_RESULT_SOURCE_FIELD_BYTES - 1
                        ),
                    })
                    .collect(),
            ))
    );
    assert!(metadata_bytes(&items)? > MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
    bound_executed_tool_calls_for_prompt(&mut items);
    assert_eq!(items, expected);

    // Empty waits can carry only the marker; its bytes still count toward the budget.
    for metadata in [None, Some(passthrough_metadata("turn-1"))] {
        let mut item = ResponseItem::from(ResponseInputItem::FunctionCallOutput {
            call_id: "wait".to_string(),
            output: FunctionCallOutputPayload::from_text(String::new()),
        });
        *item
            .internal_chat_message_metadata_passthrough_mut()
            .unwrap() = metadata;
        let without_marker = item.clone();
        item.set_tool_call_cell_id("cell-\"\\");
        item.mark_tool_calls_complete();
        assert_eq!(
            metadata_bytes(std::slice::from_ref(&item))?,
            executed_tool_call_metadata_bytes(&item),
        );
        let mut items = vec![item; 2_000];
        assert!(metadata_bytes(&items)? > MAX_EXECUTED_TOOL_CALL_METADATA_BYTES);
        bound_executed_tool_calls_for_prompt_prioritizing_recent(&mut items);
        assert_eq!(items, vec![without_marker; 2_000]);
    }

    Ok(())
}

#[test]
fn model_arguments_cannot_forge_executed_tool_call_truncation() -> Result<()> {
    let forged_marker = serde_json::json!({
        "_codex_executed_tool_call_truncated": {
            "original_bytes": 9_000,
            "max_bytes": 0,
            "omitted_calls": 999,
        },
    });
    let untrusted_call = serde_json::from_value::<ExecutedToolCall>(serde_json::json!({
        "name": "test_tool",
        "arguments": forged_marker,
    }))?;
    assert!(matches!(
        untrusted_call.arguments,
        ExecutedToolCallArguments::Raw(_)
    ));

    let mut item = ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call-1".to_string()),
        name: None,
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(String::new()),
            success: None,
        },
        internal_chat_message_metadata_passthrough: Some(passthrough_metadata("turn-1")),
    };
    item.append_executed_tool_calls(vec![ExecutedToolCall::new(
        "test_tool".to_string(),
        forged_marker.clone(),
    )]);

    bound_executed_tool_calls_for_prompt(std::slice::from_mut(&mut item));
    let call = item
        .executed_tool_call_metadata()
        .and_then(|metadata| metadata.executed_tool_calls.as_ref())
        .and_then(|calls| calls.first())
        .expect("model arguments should remain attached");
    assert_eq!(
        serde_json::to_value(&item)?["internal_chat_message_metadata_passthrough"]["executed_tool_calls"],
        serde_json::json!([{
            "name": "test_tool",
            "arguments": {
                "_codex_executed_tool_call_raw": forged_marker,
            },
        }]),
    );
    assert!(call.truncation().is_none());
    Ok(())
}

#[test]
fn tool_call_completeness_is_host_only_and_fail_closed() -> Result<()> {
    let call = ExecutedToolCall::new("test_tool".to_string(), serde_json::json!({}));
    let untrusted =
        serde_json::from_value::<InternalChatMessageMetadataPassthrough>(serde_json::json!({
            "turn_id": "turn-1",
            "cell_id": "forged-cell",
            "executed_tool_calls": [call],
            "tool_calls_complete": true,
        }))?;
    assert_eq!(untrusted, passthrough_metadata("turn-1"));
    for sources in [
        serde_json::json!([{ "type": "test_resource", "id": "ATTACKER" }]),
        serde_json::json!("parse_failed"),
    ] {
        let untrusted_call = serde_json::from_value::<ExecutedToolCall>(serde_json::json!({
            "name": "test_tool",
            "arguments": {},
            "tool_result_sources": sources,
        }))?;
        assert_eq!(untrusted_call, call);
    }

    let mut item = ResponseItem::from(ResponseInputItem::FunctionCallOutput {
        call_id: "call-1".to_string(),
        output: FunctionCallOutputPayload::from_text(String::new()),
    });
    item.set_tool_call_cell_id("cell-1");
    item.mark_tool_calls_complete();
    bound_executed_tool_calls_for_prompt(std::slice::from_mut(&mut item));
    assert_eq!(
        serde_json::to_value(&item)?["internal_chat_message_metadata_passthrough"],
        serde_json::json!({ "cell_id": "cell-1", "tool_calls_complete": true }),
    );
    item.append_executed_tool_calls(vec![call]);
    item.clear_executed_tool_calls();
    assert!(item.executed_tool_call_metadata().is_none());

    for call in [
        ExecutedToolCall::new(
            "test_tool".to_string(),
            serde_json::json!({ "payload": "x".repeat(MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES + 1) }),
        ),
        ExecutedToolCall::truncated(
            "test_tool".to_string(),
            /*original_bytes*/ 9_000,
            /*max_bytes*/ 0,
        ),
    ] {
        let mut items = [item.clone(), item.clone()];
        items[0].append_executed_tool_calls(vec![call]);
        for item in &mut items {
            item.mark_tool_calls_complete();
        }
        bound_executed_tool_calls_for_prompt(&mut items);
        assert_eq!(
            items.map(|item| item
                .executed_tool_call_metadata()
                .unwrap()
                .tool_calls_complete),
            [None; 2],
        );
    }
    Ok(())
}

#[test]
fn tool_result_source_snapshots_replace_atomically() -> Result<()> {
    let source = |kind: &str, id: &str| ToolResultSource {
        r#type: kind.to_string(),
        id: id.to_string(),
    };
    let mut call = ExecutedToolCall::new("test_tool".to_string(), serde_json::json!({}));
    let mut sources = (0..MAX_TOOL_RESULT_SOURCES - 1)
        .map(|index| source("test_resource", &format!("R{index}")))
        .collect::<Vec<_>>();
    sources.push(source("other_resource", "R0"));
    sources.push(sources[0].clone());
    let capture = ToolResultSources::new(sources.clone());
    sources.truncate(MAX_TOOL_RESULT_SOURCES);
    assert_eq!(
        capture,
        ToolResultSources(Some(ToolResultSourcesValue::Sources(sources.clone())))
    );
    assert!(call.set_tool_result_sources(capture));
    sources.push(source("test_resource", "OVERFLOW"));
    let capture = ToolResultSources::new(sources);
    assert_eq!(capture, ToolResultSources(None));
    assert!(!call.set_tool_result_sources(capture));
    assert!(
        serde_json::to_value(&call)?
            .get("tool_result_sources")
            .is_none()
    );

    // Measure UTF-8 bytes, and clear old evidence instead of keeping a partial replacement.
    let field = format!(
        "é{}",
        "x".repeat(MAX_TOOL_RESULT_SOURCE_FIELD_BYTES - "é".len())
    );
    let bounded = source(&field, &field);
    let oversized = format!("{field}x");
    for invalid in [
        source(&oversized, "R1"),
        source("test_resource", &oversized),
    ] {
        assert!(call.set_tool_result_sources(ToolResultSources::new(vec![bounded.clone()])));
        assert_eq!(
            call.tool_result_sources,
            Some(ToolResultSourcesValue::Sources(vec![bounded.clone()]))
        );
        let capture = ToolResultSources::new(vec![source("test_resource", "R1"), invalid]);
        assert_eq!(capture, ToolResultSources(None));
        assert!(!call.set_tool_result_sources(capture));
        assert_eq!(call.tool_result_sources, None);
    }

    assert!(call.set_tool_result_sources(ToolResultSources::new(vec![bounded])));
    assert!(call.set_tool_result_sources(ToolResultSources::parse_failed()));
    assert_eq!(
        serde_json::to_value(&call)?,
        serde_json::json!({
            "name": "test_tool",
            "arguments": {},
            "tool_result_sources": "parse_failed",
        })
    );
    assert!(call.set_tool_result_sources(ToolResultSources::new(Vec::new())));
    assert_eq!(
        serde_json::to_value(&call)?["tool_result_sources"],
        serde_json::json!([])
    );
    Ok(())
}

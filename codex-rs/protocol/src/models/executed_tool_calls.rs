use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

use super::InternalChatMessageMetadataPassthrough;
use super::ResponseItem;

const MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES: usize = 8 * 1024;
/// Maximum distinct result sources retained for one tool invocation.
const MAX_TOOL_RESULT_SOURCES: usize = 32;
/// Maximum UTF-8 bytes for each source's `type` and `id` separately, not the source list.
pub const MAX_TOOL_RESULT_SOURCE_FIELD_BYTES: usize = 128;
/// Maximum serialized warehouse-only attempted-tool metadata in one request.
const MAX_EXECUTED_TOOL_CALL_METADATA_BYTES: usize = 32 * 1024;
const EXECUTED_TOOL_CALL_METADATA_FIELD_BYTES: usize = b"\"executed_tool_calls\":".len();
const INTERNAL_CHAT_MESSAGE_METADATA_PASSTHROUGH_FIELD_BYTES: usize =
    b"\"internal_chat_message_metadata_passthrough\":".len();

fn executed_tool_call_metadata_field_bytes(
    metadata: &InternalChatMessageMetadataPassthrough,
) -> usize {
    let fields = InternalChatMessageMetadataPassthrough {
        cell_id: metadata.cell_id.clone(),
        tool_calls_complete: metadata.tool_calls_complete,
        ..Default::default()
    };
    let mut bytes =
        serde_json::to_vec(&fields).map_or(usize::MAX, |fields| fields.len().saturating_sub(2));
    if metadata.executed_tool_calls.is_some() {
        bytes = bytes
            .saturating_add(usize::from(bytes > 0))
            .saturating_add(EXECUTED_TOOL_CALL_METADATA_FIELD_BYTES);
    }
    if bytes == 0 {
        0
    } else if metadata.turn_id.is_some()
        || metadata.create_time.is_some()
        || metadata.content_item_kinds.is_some()
    {
        bytes + 1
    } else {
        bytes + INTERNAL_CHAT_MESSAGE_METADATA_PASSTHROUGH_FIELD_BYTES + 3
    }
}

/// Returns the exact serialized wire size of an item's attempted-tool metadata.
pub fn executed_tool_call_metadata_bytes(item: &ResponseItem) -> usize {
    let Some(metadata) = item.executed_tool_call_metadata() else {
        return 0;
    };
    metadata
        .executed_tool_calls
        .as_ref()
        .map_or(0, |calls| {
            serde_json::to_vec(calls)
                .map(|calls| calls.len())
                .unwrap_or(usize::MAX)
        })
        .saturating_add(executed_tool_call_metadata_field_bytes(metadata))
}

impl InternalChatMessageMetadataPassthrough {
    /// Compares call order, names and arguments, ignoring optional source metadata.
    pub fn has_same_tool_calls(&self, calls: &[ExecutedToolCall]) -> bool {
        self.executed_tool_calls.as_ref().is_some_and(|recorded| {
            recorded.len() == calls.len()
                && recorded.iter().zip(calls).all(|(recorded, call)| {
                    recorded.name == call.name && recorded.arguments() == call.arguments()
                })
        })
    }
}

/// Bounds attempted-tool metadata fairly across the complete serialized request.
pub fn bound_executed_tool_calls_for_prompt(items: &mut [ResponseItem]) {
    bound_executed_tool_calls_for_prompt_with_priority(items, /*prioritize_recent*/ false);
}

/// Bounds retained history without letting older calls displace the newest calls.
pub fn bound_executed_tool_calls_for_prompt_prioritizing_recent(items: &mut [ResponseItem]) {
    items.reverse();
    bound_executed_tool_calls_for_prompt_with_priority(items, /*prioritize_recent*/ true);
    items.reverse();
}

fn bound_executed_tool_calls_for_prompt_with_priority(
    items: &mut [ResponseItem],
    prioritize_recent: bool,
) {
    let mut remaining_items = 0_usize;
    let mut original_calls = 0_usize;
    let mut original_metadata_bytes = 0_usize;
    let mut truncated = false;

    for item in items.iter_mut() {
        let Some(calls) = item
            .internal_chat_message_metadata_passthrough_mut()
            .and_then(Option::as_mut)
            .and_then(|metadata| metadata.executed_tool_calls.as_mut())
            .filter(|calls| !calls.is_empty())
        else {
            original_metadata_bytes =
                original_metadata_bytes.saturating_add(executed_tool_call_metadata_bytes(item));
            continue;
        };
        for call in calls {
            let argument_bytes = serde_json::to_vec(&call.arguments)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX);
            if call.truncation().is_none() && argument_bytes > MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES
            {
                call.set_truncation(
                    argument_bytes,
                    MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES,
                    /*omitted_calls*/ None,
                );
            }
            truncated |= call.truncation().is_some();
            original_calls = original_calls.saturating_add(1).saturating_add(
                call.truncation()
                    .and_then(|truncation| truncation.omitted_calls)
                    .unwrap_or_default(),
            );
        }
        remaining_items += 1;
        original_metadata_bytes =
            original_metadata_bytes.saturating_add(executed_tool_call_metadata_bytes(item));
    }

    // Source evidence is optional; dropping it must not discard calls or their completion proof.
    if original_metadata_bytes > MAX_EXECUTED_TOOL_CALL_METADATA_BYTES {
        original_metadata_bytes = 0;
        for item in items.iter_mut() {
            if let Some(metadata) = item
                .internal_chat_message_metadata_passthrough_mut()
                .and_then(Option::as_mut)
            {
                for call in metadata.executed_tool_calls.iter_mut().flatten() {
                    call.tool_result_sources = None;
                }
            }
            original_metadata_bytes =
                original_metadata_bytes.saturating_add(executed_tool_call_metadata_bytes(item));
        }
    }

    // A terminal marker can be on an empty wait output, separate from the lost calls.
    if truncated || original_metadata_bytes > MAX_EXECUTED_TOOL_CALL_METADATA_BYTES {
        for item in items.iter_mut() {
            let item_bytes = executed_tool_call_metadata_bytes(item);
            if let Some(metadata) = item
                .internal_chat_message_metadata_passthrough_mut()
                .and_then(Option::as_mut)
                && metadata.tool_calls_complete == Some(true)
            {
                metadata.tool_calls_complete = None;
                original_metadata_bytes = original_metadata_bytes.saturating_sub(
                    item_bytes.saturating_sub(executed_tool_call_metadata_bytes(item)),
                );
            }
        }
    }
    if original_metadata_bytes <= MAX_EXECUTED_TOOL_CALL_METADATA_BYTES {
        return;
    }

    // Drop marker-only waits before spending the budget on recorded calls.
    for item in items.iter_mut() {
        if item.executed_tool_call_metadata().is_some_and(|metadata| {
            metadata
                .executed_tool_calls
                .as_ref()
                .is_none_or(Vec::is_empty)
        }) {
            original_metadata_bytes =
                original_metadata_bytes.saturating_sub(executed_tool_call_metadata_bytes(item));
            item.clear_executed_tool_calls();
        }
    }
    if original_metadata_bytes <= MAX_EXECUTED_TOOL_CALL_METADATA_BYTES {
        return;
    }

    let overflow_fallback = items.iter().enumerate().find_map(|(index, item)| {
        item.executed_tool_call_metadata().and_then(|metadata| {
            metadata
                .executed_tool_calls
                .as_ref()
                .and_then(|calls| calls.first())
                .map(|call| (index, call.clone(), metadata.cell_id.clone()))
        })
    });

    let omitted_call_reservation = serde_json::to_vec(&serde_json::json!({
        "_codex_executed_tool_call_truncated": ExecutedToolCallTruncation {
            original_bytes: usize::MAX,
            max_bytes: usize::MAX,
            omitted_calls: Some(usize::MAX),
            original_name_bytes: Some(usize::MAX),
        },
    }))
    .map(|bytes| bytes.len())
    .unwrap_or(usize::MAX);
    let mut remaining_bytes =
        MAX_EXECUTED_TOOL_CALL_METADATA_BYTES.saturating_sub(omitted_call_reservation);
    for item in items.iter_mut() {
        let Some(metadata) = item.executed_tool_call_metadata() else {
            continue;
        };
        if metadata
            .executed_tool_calls
            .as_ref()
            .is_none_or(Vec::is_empty)
        {
            continue;
        }

        let metadata_field_bytes = executed_tool_call_metadata_field_bytes(metadata);
        let item_budget = if prioritize_recent {
            remaining_bytes
        } else {
            remaining_bytes / remaining_items
        };
        item.bound_executed_tool_calls_with_budget(
            item_budget.saturating_sub(metadata_field_bytes),
        );
        remaining_bytes = remaining_bytes.saturating_sub(executed_tool_call_metadata_bytes(item));
        remaining_items -= 1;
    }

    let represented_calls = items
        .iter()
        .filter_map(ResponseItem::executed_tool_call_metadata)
        .filter_map(|metadata| metadata.executed_tool_calls.as_ref())
        .flatten()
        .map(|call| {
            1 + call
                .truncation()
                .and_then(|truncation| truncation.omitted_calls)
                .unwrap_or_default()
        })
        .sum::<usize>();
    if represented_calls == original_calls {
        return;
    }

    if represented_calls == 0 {
        if let Some((index, mut call, cell_id)) = overflow_fallback {
            let original_bytes = call
                .truncation()
                .map(|truncation| truncation.original_bytes)
                .unwrap_or_else(|| {
                    serde_json::to_vec(&call.arguments)
                        .map(|bytes| bytes.len())
                        .unwrap_or(usize::MAX)
                });
            let original_name_bytes = call.name.len();
            let name_boundary = call.name.floor_char_boundary(
                original_name_bytes.min(MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES / 2),
            );
            call.name.truncate(name_boundary);
            call.set_truncation_with_name(
                original_bytes,
                /*max_bytes*/ 0,
                Some(original_calls.saturating_sub(1)),
                (name_boundary < original_name_bytes).then_some(original_name_bytes),
            );
            items[index].append_executed_tool_calls(vec![call]);
            if let Some(cell_id) = cell_id {
                items[index].set_tool_call_cell_id(&cell_id);
            }
        }
        return;
    }

    let omission_call = if prioritize_recent {
        items.iter_mut().rev().find_map(first_executed_tool_call)
    } else {
        items.iter_mut().find_map(first_executed_tool_call)
    };
    if let Some(call) = omission_call {
        let original_bytes = call
            .truncation()
            .map(|truncation| truncation.original_bytes)
            .unwrap_or_else(|| {
                serde_json::to_vec(&call.arguments)
                    .map(|bytes| bytes.len())
                    .unwrap_or(usize::MAX)
            });
        let max_bytes = call
            .truncation()
            .map(|truncation| truncation.max_bytes)
            .unwrap_or_default();
        let previous_omissions = call
            .truncation()
            .and_then(|truncation| truncation.omitted_calls)
            .unwrap_or_default();
        call.set_truncation(
            original_bytes,
            max_bytes,
            Some(
                previous_omissions.saturating_add(original_calls.saturating_sub(represented_calls)),
            ),
        );
    }
}

fn first_executed_tool_call(item: &mut ResponseItem) -> Option<&mut ExecutedToolCall> {
    item.internal_chat_message_metadata_passthrough_mut()
        .and_then(Option::as_mut)
        .and_then(|metadata| metadata.executed_tool_calls.as_mut())
        .and_then(|calls| calls.first_mut())
}

/// Raw model arguments or trusted truncation metadata for an attempted tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(untagged)]
pub enum ExecutedToolCallArguments {
    Raw(serde_json::Value),
    #[serde(skip_deserializing)]
    Truncated {
        #[serde(rename = "_codex_executed_tool_call_truncated")]
        truncation: ExecutedToolCallTruncation,
    },
}

/// A model-attempted Codex tool invocation captured at the shared runtime boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
pub struct ExecutedToolCall {
    pub name: String,
    #[ts(type = "unknown")]
    arguments: ExecutedToolCallArguments,
    /// Host-generated analytics only: ignore input JSON rather than accepting caller-supplied
    /// evidence, and keep this out of public schemas and generated clients.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    #[ts(skip)]
    tool_result_sources: Option<ToolResultSourcesValue>,
}

/// A bounded capture update. Omitted updates still clear any previously recorded evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResultSources(Option<ToolResultSourcesValue>);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
enum ToolResultSourcesValue {
    #[serde(rename = "parse_failed")]
    ParseFailed,
    // Preserve the existing array shape for successful captures, including an empty array.
    #[serde(untagged)]
    Sources(Vec<ToolResultSource>),
}

impl ToolResultSources {
    /// Deduplicates a complete source list, discarding all sources if it exceeds a limit.
    pub fn new(sources: Vec<ToolResultSource>) -> Self {
        let mut unique_sources = Vec::new();
        for source in sources {
            if unique_sources.contains(&source) {
                continue;
            }
            if unique_sources.len() == MAX_TOOL_RESULT_SOURCES
                || source.r#type.len() > MAX_TOOL_RESULT_SOURCE_FIELD_BYTES
                || source.id.len() > MAX_TOOL_RESULT_SOURCE_FIELD_BYTES
            {
                return Self(None);
            }
            unique_sources.push(source);
        }
        Self(Some(ToolResultSourcesValue::Sources(unique_sources)))
    }

    /// Records that source parsing was attempted but failed, not that a budget was exceeded.
    pub fn parse_failed() -> Self {
        Self(Some(ToolResultSourcesValue::ParseFailed))
    }
}

/// A trusted source identity observed in an accepted tool result by the host.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolResultSource {
    #[serde(rename = "type")]
    pub r#type: String,
    pub id: String,
}

/// Trusted truncation details generated locally for an oversized attempted tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
pub struct ExecutedToolCallTruncation {
    original_bytes: usize,
    max_bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    omitted_calls: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_name_bytes: Option<usize>,
}

impl ExecutedToolCall {
    /// Creates a recorded call without treating model-provided JSON as trusted metadata.
    pub fn new(name: String, arguments: serde_json::Value) -> Self {
        let arguments = if arguments
            .as_object()
            .is_some_and(|object| object.contains_key("_codex_executed_tool_call_truncated"))
        {
            serde_json::json!({ "_codex_executed_tool_call_raw": arguments })
        } else {
            arguments
        };
        Self {
            name,
            arguments: ExecutedToolCallArguments::Raw(arguments),
            tool_result_sources: None,
        }
    }

    /// Replaces oversized arguments with internally generated truncation metadata.
    pub fn truncated(name: String, original_bytes: usize, max_bytes: usize) -> Self {
        let mut call = Self::new(name, serde_json::Value::Null);
        call.set_truncation(original_bytes, max_bytes, /*omitted_calls*/ None);
        call
    }

    /// Returns the raw arguments or locally generated truncation payload.
    pub fn arguments(&self) -> &ExecutedToolCallArguments {
        &self.arguments
    }

    /// Replaces this invocation's capture outcome, including clearing omitted evidence.
    pub fn set_tool_result_sources(&mut self, sources: ToolResultSources) -> bool {
        self.tool_result_sources = sources.0;
        self.tool_result_sources.is_some()
    }

    fn truncation(&self) -> Option<&ExecutedToolCallTruncation> {
        match &self.arguments {
            ExecutedToolCallArguments::Raw(_) => None,
            ExecutedToolCallArguments::Truncated { truncation } => Some(truncation),
        }
    }

    fn set_truncation(
        &mut self,
        original_bytes: usize,
        max_bytes: usize,
        omitted_calls: Option<usize>,
    ) {
        self.set_truncation_with_name(
            original_bytes,
            max_bytes,
            omitted_calls,
            /*original_name_bytes*/ None,
        );
    }

    fn set_truncation_with_name(
        &mut self,
        original_bytes: usize,
        max_bytes: usize,
        omitted_calls: Option<usize>,
        original_name_bytes: Option<usize>,
    ) {
        self.arguments = ExecutedToolCallArguments::Truncated {
            truncation: ExecutedToolCallTruncation {
                original_bytes,
                max_bytes,
                omitted_calls,
                original_name_bytes,
            },
        };
    }
}

impl ResponseItem {
    fn ensure_tool_call_metadata(&mut self) -> Option<&mut InternalChatMessageMetadataPassthrough> {
        self.internal_chat_message_metadata_passthrough_mut()
            .map(Option::get_or_insert_default)
    }

    /// Associates host-recorded calls and completeness with their Code Mode cell.
    pub fn set_tool_call_cell_id(&mut self, cell_id: &str) {
        if let Some(metadata) = self.ensure_tool_call_metadata() {
            metadata.cell_id = Some(cell_id.to_string());
        }
    }

    /// Attaches model-attempted tool invocations without replacing existing item metadata.
    pub fn append_executed_tool_calls(&mut self, calls: Vec<ExecutedToolCall>) {
        if calls.is_empty() {
            return;
        }
        let Some(metadata) = self.ensure_tool_call_metadata() else {
            return;
        };
        metadata
            .executed_tool_calls
            .get_or_insert_with(Vec::new)
            .extend(calls);
    }

    /// Marks a host-owned cell's recorded tool calls as complete.
    pub fn mark_tool_calls_complete(&mut self) {
        if let Some(metadata) = self.ensure_tool_call_metadata() {
            metadata.tool_calls_complete = Some(true);
        }
    }

    /// Returns warehouse-only attempted-tool metadata for any supported item variant.
    pub fn executed_tool_call_metadata(&self) -> Option<&InternalChatMessageMetadataPassthrough> {
        self.internal_chat_message_metadata_passthrough()
    }

    /// Bounds one request item's attempted calls to its share of the prompt budget.
    fn bound_executed_tool_calls_with_budget(&mut self, max_metadata_bytes: usize) {
        let Some(metadata) = self.internal_chat_message_metadata_passthrough_mut() else {
            return;
        };
        let Some(passthrough) = metadata.as_mut() else {
            return;
        };
        let Some(calls) = passthrough.executed_tool_calls.as_mut() else {
            return;
        };

        let mut serialized_bytes = 2;
        let mut retained_calls = 0;
        calls.retain_mut(|call| {
            let separator_bytes = usize::from(retained_calls > 0);
            let remaining_bytes = max_metadata_bytes
                .saturating_sub(serialized_bytes)
                .saturating_sub(separator_bytes);
            let argument_bytes = serde_json::to_vec(&call.arguments)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX);
            let call_bytes = serde_json::to_vec(&*call)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX);

            if call_bytes > remaining_bytes
                || argument_bytes > MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES
            {
                let original_bytes = call
                    .truncation()
                    .map(|truncation| truncation.original_bytes)
                    .unwrap_or(argument_bytes);
                let omitted_calls = call
                    .truncation()
                    .and_then(|truncation| truncation.omitted_calls);
                call.set_truncation(
                    original_bytes,
                    remaining_bytes.min(MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES),
                    omitted_calls,
                );
            }

            let call_bytes = serde_json::to_vec(&*call)
                .map(|bytes| bytes.len())
                .unwrap_or(usize::MAX);
            if call_bytes > remaining_bytes {
                return false;
            }

            serialized_bytes = serialized_bytes
                .saturating_add(separator_bytes)
                .saturating_add(call_bytes);
            retained_calls += 1;
            true
        });

        if calls.is_empty() {
            self.clear_executed_tool_calls();
        }
    }

    /// Removes untrusted warehouse-only tool records without changing the turn ID.
    pub fn clear_executed_tool_calls(&mut self) {
        let Some(metadata) = self.internal_chat_message_metadata_passthrough_mut() else {
            return;
        };
        let Some(passthrough) = metadata.as_mut() else {
            return;
        };
        passthrough.cell_id = None;
        passthrough.executed_tool_calls = None;
        passthrough.tool_calls_complete = None;
        if *passthrough == InternalChatMessageMetadataPassthrough::default() {
            *metadata = None;
        }
    }
}

#[cfg(test)]
#[path = "executed_tool_calls_tests.rs"]
mod tests;

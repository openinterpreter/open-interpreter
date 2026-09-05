use std::collections::HashMap;
use std::collections::HashSet;

use codex_code_mode::CellId;
use codex_protocol::models::ExecutedToolCall;
use codex_protocol::models::ExecutedToolCallArguments;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::bound_executed_tool_calls_for_prompt_prioritizing_recent;
use codex_protocol::models::executed_tool_call_metadata_bytes;
use codex_protocol::openai_models::ToolMode;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use crate::tools::context::ToolCallSource;
use crate::tools::context::ToolPayload;
use crate::tools::router::ToolCall;
use crate::utils::json::serialized_json_bytes;

const MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_EXECUTED_TOOL_CALL_FULL_ARGUMENT_BYTES_PER_OUTPUT: usize = 32 * 1024;
const MAX_PENDING_EXECUTED_TOOL_CALLS: usize = 256;

/// Best-effort, session-scoped attempted-tool metadata; cancellation, compaction,
/// and yielded cells without another wait can leave pending calls unreported.
#[derive(Default)]
pub(crate) struct ExecutedToolCallRecorder {
    state: std::sync::Mutex<ExecutedToolCallRecorderState>,
}

#[derive(Default)]
struct ExecutedToolCallRecorderState {
    direct_calls: HashMap<String, ExecutedToolCall>,
    cells: HashMap<CellId, RecordedCell>,
    output_cells: HashMap<String, CellId>,
    retained_calls: HashMap<(std::mem::Discriminant<ResponseItem>, String), RetainedToolCalls>,
    pending_nested_calls: usize,
}

/// Keep each output's calls and completion marker together through replay and pruning.
#[derive(Default)]
struct RetainedToolCalls {
    calls: Vec<ExecutedToolCall>,
    complete: bool,
    cell_id: Option<String>,
    runtime_cell_id: Option<CellId>,
    // Invocation IDs stay local and are retained only with their original output's calls.
    call_index_by_id: HashMap<String, usize>,
    sources_updated: bool,
}

#[derive(Default, PartialEq, Eq)]
enum CellCompletion {
    #[default]
    Unobserved,
    Started,
    Recording,
    Incomplete,
    Complete,
}

#[derive(Default)]
struct RecordedCell {
    pending_calls: IndexMap<String, ExecutedToolCall>,
    pending_full_argument_bytes: usize,
    completion: CellCompletion,
    originating_call_id: Option<String>,
}

impl ExecutedToolCallRecorderState {
    fn register_cell(&mut self, cell_id: &CellId, output_call_id: &str) {
        if self.cells.len() >= MAX_PENDING_EXECUTED_TOOL_CALLS && !self.cells.contains_key(cell_id)
        {
            let output_cells = self.output_cells.values().collect::<HashSet<_>>();
            let finished_cell = self.cells.iter().find_map(|(id, cell)| {
                // A finished cell can still have missing or truncated tool call records.
                (matches!(
                    cell.completion,
                    CellCompletion::Complete | CellCompletion::Incomplete
                ) && cell.pending_calls.is_empty()
                    && !output_cells.contains(id))
                .then(|| id.clone())
            });
            if let Some(id) = finished_cell {
                self.cells.remove(&id);
            }
        }
        if (self.cells.len() >= MAX_PENDING_EXECUTED_TOOL_CALLS
            && !self.cells.contains_key(cell_id))
            || (self.output_cells.len() >= MAX_PENDING_EXECUTED_TOOL_CALLS
                && !self.output_cells.contains_key(output_call_id))
        {
            return;
        }
        self.cells
            .entry(cell_id.clone())
            .or_default()
            .originating_call_id
            .get_or_insert_with(|| output_call_id.to_string());
        self.output_cells
            .insert(output_call_id.to_string(), cell_id.clone());
    }
}

impl ExecutedToolCallRecorder {
    pub(crate) fn record_tool_call(
        &self,
        call: &ToolCall,
        source: &ToolCallSource,
        tool_mode: ToolMode,
    ) {
        if matches!(source, ToolCallSource::Direct)
            && matches!(tool_mode, ToolMode::CodeMode | ToolMode::CodeModeOnly)
            && call.tool_name.is_default_namespace()
            && matches!(
                (call.tool_name.name.as_str(), &call.payload),
                (
                    crate::tools::code_mode::PUBLIC_TOOL_NAME,
                    ToolPayload::Custom { .. }
                ) | (
                    crate::tools::code_mode::WAIT_TOOL_NAME,
                    ToolPayload::Function { .. }
                )
            )
        {
            return;
        }

        let original_bytes = match &call.payload {
            ToolPayload::Function { arguments } => arguments.len(),
            ToolPayload::Custom { input } => serialized_json_bytes(input).unwrap_or(usize::MAX),
            ToolPayload::ToolSearch { arguments } => {
                serialized_json_bytes(arguments).unwrap_or(usize::MAX)
            }
        };
        let name = codex_tools::code_mode_name_for_tool_name(&call.tool_name);
        let recorded_call = if original_bytes > MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES {
            ExecutedToolCall::truncated(name, original_bytes, MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES)
        } else {
            let arguments = match &call.payload {
                ToolPayload::Function { arguments } => serde_json::from_str(arguments)
                    .unwrap_or_else(|_| JsonValue::String(arguments.clone())),
                ToolPayload::Custom { input } => JsonValue::String(input.clone()),
                ToolPayload::ToolSearch { arguments } => {
                    serde_json::to_value(arguments).unwrap_or_default()
                }
            };
            ExecutedToolCall::new(name, arguments)
        };
        match source {
            ToolCallSource::Direct | ToolCallSource::DirectPlaintextMessage => {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if state.direct_calls.len() < MAX_PENDING_EXECUTED_TOOL_CALLS {
                    state
                        .direct_calls
                        .entry(call.call_id.clone())
                        .or_insert(recorded_call);
                } else if state.direct_calls.len() == MAX_PENDING_EXECUTED_TOOL_CALLS
                    && !state.direct_calls.contains_key(&call.call_id)
                {
                    state.direct_calls.insert(
                        call.call_id.clone(),
                        ExecutedToolCall::truncated(
                            recorded_call.name,
                            original_bytes,
                            /*max_bytes*/ 0,
                        ),
                    );
                }
            }
            ToolCallSource::CodeMode { cell_id, .. } => {
                self.record_nested_tool_call(
                    CellId::new(cell_id.clone()),
                    call.call_id.clone(),
                    recorded_call,
                    original_bytes,
                );
            }
        }
    }

    fn record_nested_tool_call(
        &self,
        cell_id: CellId,
        call_id: String,
        call: ExecutedToolCall,
        original_bytes: usize,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.pending_nested_calls > MAX_PENDING_EXECUTED_TOOL_CALLS
            || (state.cells.len() >= MAX_PENDING_EXECUTED_TOOL_CALLS
                && !state.cells.contains_key(&cell_id))
        {
            if let Some(cell) = state.cells.get_mut(&cell_id) {
                cell.completion = CellCompletion::Incomplete;
            }
            return;
        }
        let at_pending_call_limit = state.pending_nested_calls == MAX_PENDING_EXECUTED_TOOL_CALLS;
        let cell = state.cells.entry(cell_id).or_default();
        let max_bytes = MAX_EXECUTED_TOOL_CALL_ARGUMENT_BYTES.min(
            MAX_EXECUTED_TOOL_CALL_FULL_ARGUMENT_BYTES_PER_OUTPUT
                .saturating_sub(cell.pending_full_argument_bytes),
        );
        let call = if at_pending_call_limit {
            ExecutedToolCall::truncated(call.name, original_bytes, /*max_bytes*/ 0)
        } else if original_bytes <= max_bytes {
            cell.pending_full_argument_bytes = cell
                .pending_full_argument_bytes
                .saturating_add(original_bytes);
            call
        } else {
            ExecutedToolCall::truncated(call.name, original_bytes, max_bytes)
        };
        cell.completion = if matches!(
            cell.completion,
            CellCompletion::Started | CellCompletion::Recording
        ) && !matches!(
            call.arguments(),
            ExecutedToolCallArguments::Truncated { .. }
        ) {
            CellCompletion::Recording
        } else {
            CellCompletion::Incomplete
        };
        cell.pending_calls.insert(call_id, call);
        state.pending_nested_calls += 1;
    }

    pub(crate) fn record_tool_result_sources(
        &self,
        source: &ToolCallSource,
        call_id: &str,
        result_sources: codex_protocol::models::ToolResultSources,
    ) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let call = match source {
            ToolCallSource::Direct | ToolCallSource::DirectPlaintextMessage => {
                state.direct_calls.get_mut(call_id)
            }
            ToolCallSource::CodeMode { cell_id, .. } => state
                .cells
                .get_mut(&CellId::new(cell_id.clone()))
                .and_then(|cell| cell.pending_calls.get_mut(call_id)),
        };
        if let Some(call) = call {
            return call.set_tool_result_sources(result_sources);
        }
        let ToolCallSource::CodeMode { cell_id, .. } = source else {
            return false;
        };
        let Some((retained, index)) = state.retained_calls.values_mut().find_map(|retained| {
            if retained.runtime_cell_id.as_ref()?.as_str() != cell_id.as_str() {
                return None;
            }
            let index = *retained.call_index_by_id.get(call_id)?;
            Some((retained, index))
        }) else {
            return false;
        };
        // Older retry copies must not overwrite this output's accepted result metadata.
        retained.sources_updated = true;
        retained.calls[index].set_tool_result_sources(result_sources)
    }

    pub(crate) fn attach_pending_to_prompt(
        &self,
        items: &mut [ResponseItem],
        retry_cache: &mut HashMap<
            (std::mem::Discriminant<ResponseItem>, String),
            Vec<ExecutedToolCall>,
        >,
    ) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.direct_calls.is_empty()
            && state.output_cells.is_empty()
            && state.retained_calls.is_empty()
            && retry_cache.is_empty()
        {
            return false;
        }

        // Updated records supersede older retry snapshots.
        retry_cache.retain(|key, _| {
            !state
                .retained_calls
                .get(key)
                .is_some_and(|retained| retained.sources_updated)
        });
        let mut pending_retry_outputs = retry_cache.keys().cloned().collect::<HashSet<_>>();
        let mut pending_retained_outputs =
            state.retained_calls.keys().cloned().collect::<HashSet<_>>();
        let mut attached = false;
        let mut retained_bytes = 0_usize;
        for item in items.iter_mut().rev() {
            if state.direct_calls.is_empty()
                && state.output_cells.is_empty()
                && pending_retry_outputs.is_empty()
                && pending_retained_outputs.is_empty()
            {
                break;
            }
            let call_id = match &*item {
                ResponseItem::FunctionCallOutput {
                    call_id: Some(call_id),
                    ..
                }
                | ResponseItem::CustomToolCallOutput { call_id, .. }
                | ResponseItem::ToolSearchOutput {
                    call_id: Some(call_id),
                    ..
                } => call_id,
                _ => continue,
            };
            let key = (std::mem::discriminant(&*item), call_id.clone());
            let retained = state.retained_calls.get(&key);
            let mut complete = retained.is_some_and(|retained| retained.complete);
            let mut cell_id = retained.and_then(|retained| retained.cell_id.clone());
            let mut runtime_cell_id =
                retained.and_then(|retained| retained.runtime_cell_id.clone());
            let calls = if let Some(cached) = retry_cache.get(&key) {
                if !pending_retry_outputs.remove(&key) {
                    continue;
                }
                pending_retained_outputs.remove(&key);
                cached.clone()
            } else if let Some(retained) = retained {
                if !pending_retained_outputs.remove(&key) {
                    continue;
                }
                retained.calls.clone()
            } else {
                let mut calls = state
                    .direct_calls
                    .remove(call_id)
                    .into_iter()
                    .collect::<Vec<_>>();
                let mut call_index_by_id = HashMap::new();
                if let Some(output_cell_id) = state.output_cells.remove(call_id)
                    && let Some(cell) = state.cells.get_mut(&output_cell_id)
                {
                    cell_id = cell.originating_call_id.clone();
                    runtime_cell_id = Some(output_cell_id.clone());
                    let pending_calls = cell.pending_calls.len();
                    for (call_id, call) in cell.pending_calls.drain(..) {
                        if matches!(call.arguments(), ExecutedToolCallArguments::Raw(_)) {
                            call_index_by_id.insert(call_id, calls.len());
                        }
                        calls.push(call);
                    }
                    cell.pending_full_argument_bytes = 0;
                    complete = cell.completion == CellCompletion::Complete;
                    if matches!(
                        cell.completion,
                        CellCompletion::Complete | CellCompletion::Incomplete
                    ) {
                        state.cells.remove(&output_cell_id);
                    }
                    state.pending_nested_calls =
                        state.pending_nested_calls.saturating_sub(pending_calls);
                    state
                        .output_cells
                        .retain(|_, registered_cell_id| registered_cell_id != &output_cell_id);
                }
                if calls.is_empty() && !complete {
                    continue;
                }
                retry_cache.insert(key.clone(), calls.clone());
                state.retained_calls.insert(
                    key,
                    RetainedToolCalls {
                        calls: calls.clone(),
                        complete,
                        cell_id: cell_id.clone(),
                        runtime_cell_id,
                        call_index_by_id,
                        sources_updated: false,
                    },
                );
                calls
            };
            item.append_executed_tool_calls(calls);
            if let Some(cell_id) = cell_id {
                item.set_tool_call_cell_id(&cell_id);
            }
            if complete {
                item.mark_tool_calls_complete();
            }
            retained_bytes = retained_bytes.saturating_add(executed_tool_call_metadata_bytes(item));
            attached = true;
        }
        if !pending_retained_outputs.is_empty() {
            state
                .retained_calls
                .retain(|key, _| !pending_retained_outputs.contains(key));
        }
        if retained_bytes > MAX_EXECUTED_TOOL_CALL_FULL_ARGUMENT_BYTES_PER_OUTPUT {
            bound_executed_tool_calls_for_prompt_prioritizing_recent(items);
            let retained_before_bounding = std::mem::take(&mut state.retained_calls);
            let mut bounded_outputs = HashSet::new();
            for item in items {
                let call_id = match &*item {
                    ResponseItem::FunctionCallOutput {
                        call_id: Some(call_id),
                        ..
                    }
                    | ResponseItem::CustomToolCallOutput { call_id, .. }
                    | ResponseItem::ToolSearchOutput {
                        call_id: Some(call_id),
                        ..
                    } => call_id,
                    _ => continue,
                };
                let key = (std::mem::discriminant(&*item), call_id.clone());
                let metadata = item.executed_tool_call_metadata();
                let unique_output = bounded_outputs.insert(key.clone());
                if !unique_output && let Some(retained) = state.retained_calls.get_mut(&key) {
                    retained.call_index_by_id.clear();
                }
                let previous = retained_before_bounding.get(&key);
                if let Some(retained) = previous
                    && let Some(runtime_cell_id) = &retained.runtime_cell_id
                    && metadata
                        .is_none_or(|metadata| !metadata.has_same_tool_calls(&retained.calls))
                    && let Some(cell) = state.cells.get_mut(runtime_cell_id)
                {
                    cell.completion = CellCompletion::Incomplete;
                }
                if let Some(metadata) = metadata
                    && (metadata
                        .executed_tool_calls
                        .as_ref()
                        .is_some_and(|calls| !calls.is_empty())
                        || metadata.tool_calls_complete.is_some())
                {
                    let runtime_cell_id =
                        previous.and_then(|retained| retained.runtime_cell_id.clone());
                    // Budget rewriting or duplicate outputs invalidate the original index slots.
                    let call_index_by_id = previous
                        .filter(|retained| {
                            unique_output
                                && metadata.executed_tool_calls.as_ref() == Some(&retained.calls)
                        })
                        .map(|retained| retained.call_index_by_id.clone())
                        .unwrap_or_default();
                    let retained = state.retained_calls.entry(key).or_default();
                    retained.runtime_cell_id = runtime_cell_id;
                    retained.calls = metadata.executed_tool_calls.clone().unwrap_or_default();
                    if let Some(cell_id) = metadata.cell_id.as_ref() {
                        retained.cell_id = Some(cell_id.clone());
                    }
                    retained.complete |= metadata.tool_calls_complete == Some(true);
                    retained.call_index_by_id = call_index_by_id;
                    retained.sources_updated =
                        previous.is_some_and(|retained| retained.sources_updated);
                }
            }
        }

        attached
    }

    pub(crate) fn register_cell(&self, cell_id: &CellId, output_call_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.register_cell(cell_id, output_call_id);
    }

    pub(crate) fn start_cell(&self, cell_id: &CellId, output_call_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.register_cell(cell_id, output_call_id);
        if let Some(cell) = state.cells.get_mut(cell_id) {
            cell.completion = CellCompletion::Started;
        }
    }

    pub(crate) fn finish_cell_recording(&self, cell_id: &CellId) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cell) = state.cells.get_mut(cell_id) {
            if cell.completion == CellCompletion::Recording {
                cell.completion = CellCompletion::Complete;
            } else if cell.completion != CellCompletion::Complete && cell.pending_calls.is_empty() {
                state.cells.remove(cell_id);
            }
        }
    }
}

#[cfg(test)]
#[path = "executed_tool_calls_tests.rs"]
mod tests;

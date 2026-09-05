//! Selects backing-agent items for the canonical Voice timeline using shared rules.

use super::RealtimeHistoryState;
use super::StreamingAgentMessage;
use codex_protocol::items::AgentMessageContent;
use codex_protocol::items::DynamicToolCallStatus;
use codex_protocol::items::McpToolCallStatus;
use codex_protocol::items::TurnItem;
use codex_protocol::protocol::SubAgentActivityKind;
use codex_protocol::realtime::BemItemPresentation;
use codex_protocol::realtime::RealtimeItem;

const INLINE_MARKDOWN_DIRECTIVE: &str = "::codex-realtime-inline{}";
const INLINE_VISUALIZATION_DIRECTIVE: &str = "::codex-inline-vis{";
const VISUALIZE_DIRECTIVE: &str = "visualize{";
const BACKTICK_FENCE: &str = "```";
const TILDE_FENCE: &str = "~~~";

impl RealtimeHistoryState {
    pub(super) fn observe_item(
        &mut self,
        items: &mut Vec<RealtimeItem>,
        turn_id: &str,
        item: &TurnItem,
        completed: bool,
    ) {
        match item {
            TurnItem::AgentMessage(message) => {
                let text = message
                    .content
                    .iter()
                    .map(|content| match content {
                        AgentMessageContent::Text { text } => text.as_str(),
                    })
                    .collect::<String>();
                if !completed {
                    self.streaming_agent_message = Some(StreamingAgentMessage {
                        item_id: message.id.clone(),
                        text: text.clone(),
                    });
                }
                self.observe_assistant_message(items, turn_id, &message.id, &text);
            }
            TurnItem::ImageGeneration(image) => {
                self.add_promotion(items, turn_id, &image.id, BemItemPresentation::WholeItem);
            }
            TurnItem::Extension(extension)
                if serde_json::to_value(extension)
                    .is_ok_and(|item| item["kind"] == "image_gen.generation") =>
            {
                self.add_promotion(
                    items,
                    turn_id,
                    extension.id(),
                    BemItemPresentation::WholeItem,
                );
            }
            TurnItem::SubAgentActivity(activity)
                if completed && activity.kind == SubAgentActivityKind::Started =>
            {
                self.add_promotion(items, turn_id, &activity.id, BemItemPresentation::WholeItem);
            }
            TurnItem::DynamicToolCall(call)
                if self.active_session_id.is_some()
                    && completed
                    && call.status == DynamicToolCallStatus::Completed
                    && call.success == Some(true) =>
            {
                self.add_promotion(items, turn_id, &call.id, BemItemPresentation::WholeItem);
            }
            TurnItem::McpToolCall(call)
                if self.active_session_id.is_some()
                    && completed
                    && call.server == "codex_app"
                    && call.status == McpToolCallStatus::Completed =>
            {
                self.add_promotion(items, turn_id, &call.id, BemItemPresentation::WholeItem);
            }
            _ => {}
        }
    }

    pub(super) fn observe_assistant_message(
        &mut self,
        items: &mut Vec<RealtimeItem>,
        turn_id: &str,
        item_id: &str,
        text: &str,
    ) {
        let mut lines = text.trim_start().lines();
        let mut first = lines.next().unwrap_or_default();
        if first.starts_with('[')
            && let Some((_, content)) = first.split_once(']')
        {
            first = content.trim_start();
            if first.is_empty() {
                first = lines.next().unwrap_or_default();
            }
        }
        if first == INLINE_MARKDOWN_DIRECTIVE && text.contains('\n') {
            self.add_promotion(items, turn_id, item_id, BemItemPresentation::InlineMarkdown);
            return;
        }

        let mut in_fence = false;
        let mut visualization_index = 0;
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(BACKTICK_FENCE) || trimmed.starts_with(TILDE_FENCE) {
                in_fence = !in_fence;
                continue;
            }
            if !in_fence
                && (trimmed.starts_with(INLINE_VISUALIZATION_DIRECTIVE)
                    || trimmed.starts_with(VISUALIZE_DIRECTIVE))
            {
                self.add_promotion(
                    items,
                    turn_id,
                    item_id,
                    BemItemPresentation::InlineVisualization {
                        index: visualization_index,
                    },
                );
                visualization_index += 1;
            }
        }
    }
}

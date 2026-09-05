use std::collections::VecDeque;

use codex_extension_api::ConversationHistorySnapshot;
use codex_extension_api::ResponseItem;
pub(crate) use codex_features::GuardianV2TranscriptSource as TranscriptSource;
use codex_guardian_context::ContextTarget;
use codex_guardian_context::ConversationTranscriptConfig;
use codex_guardian_context::ConversationTranscriptEntry;
use codex_guardian_context::ConversationTranscriptEntryKind;
use codex_guardian_context::ConversationTranscriptOptions;
use codex_guardian_context::GuardianRootMessage;
#[cfg(test)]
use codex_guardian_context::MANUAL_APPROVAL_DEVELOPER_PREFIX;
use codex_guardian_context::SectionError;
use codex_guardian_context::SectionHistory;
use codex_guardian_context::SectionInput;
use codex_guardian_context::TranscriptEntryLimits;
use codex_guardian_context::TranscriptRetentionConfig;
use codex_guardian_context::default_registry;
pub(crate) use codex_guardian_context::truncate_text as truncate_entry;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ImageDetail;
use codex_protocol::protocol::TruncationPolicy;

use self::window::TranscriptWindow;
use super::truncation::TruncationObservation;

mod window;

pub(crate) const MAX_MESSAGE_ENTRY_TOKENS: usize = 2_000;
pub(crate) const MAX_TOOL_ENTRY_TOKENS: usize = 1_000;
pub(crate) const MAX_MESSAGE_TRANSCRIPT_TOKENS: usize = 10_000;
pub(crate) const MAX_TOOL_TRANSCRIPT_TOKENS: usize = 10_000;
pub(crate) const MAX_RECENT_NON_USER_ENTRIES: usize = 40;
const MAX_TRANSCRIPT_IMAGES: usize = 4;
const MAX_TRANSCRIPT_IMAGE_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TranscriptEntryKind {
    User,
    ProtectedMessage,
    Message,
    Tool,
}

struct TranscriptEntry {
    kind: TranscriptEntryKind,
    text: String,
    tokens: usize,
    original_bytes: usize,
    retained_bytes: usize,
}

pub(crate) struct RenderedContext {
    pub(crate) authorization: Vec<String>,
    pub(crate) entries: Vec<String>,
    pub(crate) truncations: Vec<TruncationObservation>,
}

pub(crate) struct RenderedImages {
    pub(crate) images: Vec<ContentItem>,
    pub(crate) omitted_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptConfig {
    pub(crate) sources: Vec<TranscriptSource>,
    pub(crate) include_images: bool,
    pub(crate) max_message_entry_tokens: usize,
    pub(crate) max_tool_entry_tokens: usize,
    pub(crate) max_message_transcript_tokens: usize,
    pub(crate) max_tool_transcript_tokens: usize,
    pub(crate) max_recent_non_user_entries: usize,
}

impl Default for TranscriptConfig {
    fn default() -> Self {
        Self {
            sources: vec![TranscriptSource::ToolCalls, TranscriptSource::ToolOutputs],
            include_images: true,
            max_message_entry_tokens: MAX_MESSAGE_ENTRY_TOKENS,
            max_tool_entry_tokens: MAX_TOOL_ENTRY_TOKENS,
            max_message_transcript_tokens: MAX_MESSAGE_TRANSCRIPT_TOKENS,
            max_tool_transcript_tokens: MAX_TOOL_TRANSCRIPT_TOKENS,
            max_recent_non_user_entries: MAX_RECENT_NON_USER_ENTRIES,
        }
    }
}

impl TranscriptConfig {
    pub(crate) fn images<'a>(
        &self,
        items: impl IntoIterator<Item = &'a ResponseItem>,
        node_repl_images: impl IntoIterator<Item = ContentItem>,
    ) -> RenderedImages {
        if !self.include_images {
            return RenderedImages {
                images: Vec::new(),
                omitted_bytes: 0,
            };
        }

        let mut images = VecDeque::new();
        let mut image_bytes = 0usize;
        let mut omitted_bytes = 0usize;
        let mut include_image = |image_url: &str, detail: Option<ImageDetail>| {
            if image_url.len() > MAX_TRANSCRIPT_IMAGE_BYTES {
                omitted_bytes = omitted_bytes.saturating_add(image_url.len());
                return;
            }
            while images.len() >= MAX_TRANSCRIPT_IMAGES
                || image_bytes + image_url.len() > MAX_TRANSCRIPT_IMAGE_BYTES
            {
                let Some(ContentItem::InputImage { image_url, .. }) = images.pop_front() else {
                    break;
                };
                image_bytes -= image_url.len();
                omitted_bytes = omitted_bytes.saturating_add(image_url.len());
            }
            image_bytes += image_url.len();
            images.push_back(ContentItem::InputImage {
                image_url: image_url.to_owned(),
                detail,
            });
        };

        for item in items {
            match item {
                ResponseItem::Message { role, content, .. }
                    if matches!(role.as_str(), "user" | "assistant") =>
                {
                    for item in content {
                        if let ContentItem::InputImage { image_url, detail } = item {
                            include_image(image_url, *detail);
                        }
                    }
                }
                ResponseItem::FunctionCallOutput { output, .. }
                | ResponseItem::CustomToolCallOutput { output, .. }
                    if self.sources.contains(&TranscriptSource::ToolOutputs) =>
                {
                    if let Some(content) = output.content_items() {
                        for item in content {
                            if let FunctionCallOutputContentItem::InputImage { image_url, detail } =
                                item
                            {
                                include_image(image_url, *detail);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if self.sources.contains(&TranscriptSource::ToolOutputs) {
            for image in node_repl_images {
                if let ContentItem::InputImage { image_url, detail } = image {
                    include_image(&image_url, detail);
                }
            }
        }

        RenderedImages {
            images: images.into_iter().collect(),
            omitted_bytes,
        }
    }

    pub(crate) fn build_context(
        &self,
        target: ContextTarget,
        history: &dyn ConversationHistorySnapshot,
        root_conversation: &[GuardianRootMessage],
        trusted_user_answers: &[String],
    ) -> Result<RenderedContext, SectionError> {
        let history = SnapshotHistory(history);
        let retention = TranscriptRetentionConfig {
            max_message_transcript_tokens: self.max_message_transcript_tokens,
            max_tool_transcript_tokens: self.max_tool_transcript_tokens,
            max_recent_non_user_entries: self.max_recent_non_user_entries,
        };
        let transcript = ConversationTranscriptConfig {
            options: ConversationTranscriptOptions {
                include_tool_calls: self.sources.contains(&TranscriptSource::ToolCalls),
                include_tool_outputs: self.sources.contains(&TranscriptSource::ToolOutputs),
                include_reasoning: self.sources.contains(&TranscriptSource::Reasoning),
            },
            entry_limits: TranscriptEntryLimits {
                message_tokens: self.max_message_entry_tokens,
                tool_tokens: self.max_tool_entry_tokens,
                node_repl_output_tokens: self.max_tool_entry_tokens,
            },
        };
        let context = default_registry().compose(&SectionInput {
            target,
            history: &history,
            transcript: &transcript,
            root_conversation,
            trusted_user_answers,
        })?;
        let mut rendered = Self::render(context.transcript, &retention);
        rendered.authorization = context.authorization;
        Ok(rendered)
    }

    fn render(
        transcript_entries: impl IntoIterator<Item = ConversationTranscriptEntry>,
        retention: &TranscriptRetentionConfig,
    ) -> RenderedContext {
        let mut entries = Vec::new();

        for entry in transcript_entries {
            let role = entry.kind.role();
            let kind = match &entry.kind {
                ConversationTranscriptEntryKind::User => TranscriptEntryKind::User,
                ConversationTranscriptEntryKind::Developer
                | ConversationTranscriptEntryKind::ProtectedAssistant => {
                    TranscriptEntryKind::ProtectedMessage
                }
                ConversationTranscriptEntryKind::Assistant
                | ConversationTranscriptEntryKind::Reasoning => TranscriptEntryKind::Message,
                ConversationTranscriptEntryKind::ToolCall(_)
                | ConversationTranscriptEntryKind::ToolOutput(_)
                | ConversationTranscriptEntryKind::NodeReplToolOutput(_) => {
                    TranscriptEntryKind::Tool
                }
            };
            let original_bytes = entry.original_bytes;
            let text = entry.text;
            let retained_bytes = text.len();
            let entry_number = entries.len() + 1;
            let text = format!("[{entry_number}] {role}: {text}\n");
            let tokens = TruncationPolicy::Bytes(text.len()).token_budget();
            entries.push(TranscriptEntry {
                kind,
                text,
                tokens,
                original_bytes,
                retained_bytes,
            });
        }

        let mut included = vec![false; entries.len()];
        let user_messages = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (entry.kind == TranscriptEntryKind::User).then_some(
                    codex_guardian_context::UserMessageCost {
                        index,
                        tokens: entry.tokens,
                    },
                )
            })
            .collect::<Vec<_>>();
        let selection = codex_guardian_context::select_user_messages(
            &user_messages,
            retention.max_message_transcript_tokens,
        );
        for index in selection.indices {
            included[index] = true;
        }
        let available_message_tokens = retention
            .max_message_transcript_tokens
            .saturating_sub(selection.tokens);
        let mut window = TranscriptWindow::new(&entries, retention, available_message_tokens);
        for index in 0..entries.len() {
            window.insert(index);
        }

        for index in window.into_indices() {
            included[index] = true;
        }

        let mut truncations = Vec::new();
        let entries = entries
            .into_iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let component = match entry.kind {
                    TranscriptEntryKind::User => "transcript_user",
                    TranscriptEntryKind::ProtectedMessage | TranscriptEntryKind::Message => {
                        "transcript_message"
                    }
                    TranscriptEntryKind::Tool => "transcript_tool",
                };
                let retained_bytes = if included[index] {
                    entry.retained_bytes
                } else {
                    0
                };
                if entry.original_bytes > retained_bytes {
                    truncations.push(TruncationObservation {
                        component,
                        original_bytes: entry.original_bytes,
                        retained_bytes,
                    });
                }
                included[index].then_some(entry.text)
            })
            .collect();

        RenderedContext {
            authorization: Vec::new(),
            entries,
            truncations,
        }
    }
}

struct SnapshotHistory<'a>(&'a dyn ConversationHistorySnapshot);

impl SectionHistory for SnapshotHistory<'_> {
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_> {
        self.0.review_items()
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;

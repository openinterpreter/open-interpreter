//! Shared context sections for synchronous Guardian review and asynchronous scoring.
//!
//! Transcript collection and bounded host-owned history are also available directly,
//! without section composition.
//! Contributor failures abort collection without returning partial context.
//! Sections carry structured transcript evidence without depending on either
//! consumer's rendering, retention, compaction, or request lifecycle.
//! Registered contributors declare their scope once and are collected only for
//! matching context consumers. History and collection settings are borrowed for
//! each request so the default registry can be reused without retaining state.

use std::sync::Arc;
use std::sync::LazyLock;

use codex_protocol::models::ResponseItem;

use authorization::RootConversationSection;
use authorization::TrustedUserAnswersSection;
use transcript::ConversationTranscriptSection;

pub use authorization::GuardianRootMessage;
pub use composition::ComposedContext;

pub use entry::ConversationTranscriptEntry;
pub use entry::ConversationTranscriptEntryKind;
pub use history::TranscriptHistory;
pub use retention::UserMessageCost;
pub use retention::UserMessageSelection;
pub use retention::select_user_messages;
pub use transcript::ConversationTranscriptConfig;
pub use transcript::ConversationTranscriptOptions;
pub use transcript::MANUAL_APPROVAL_DEVELOPER_PREFIX;
pub use transcript::TranscriptEntryLimits;
pub use transcript::TranscriptRetentionConfig;
pub use transcript::collect_transcript;
pub use truncation::truncate_text;

mod authorization;
mod composition;
mod entry;
mod history;
mod retention;
mod transcript;
mod truncation;

/// Consumer for which a Guardian context is composed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextTarget {
    /// The reusable synchronous Guardian reviewer.
    Sync,
    /// The asynchronous Guardian action scorer.
    Async,
}

/// Consumers to which a context section contributes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectionScope {
    /// Include the section in both synchronous review and asynchronous scoring.
    Shared,
    /// Include the section only in synchronous review.
    SyncOnly,
    /// Include the section only in asynchronous scoring.
    AsyncOnly,
}

impl SectionScope {
    /// Whether this section is included for the requested context consumer.
    pub fn includes(self, target: ContextTarget) -> bool {
        match self {
            Self::Shared => true,
            Self::SyncOnly => matches!(target, ContextTarget::Sync),
            Self::AsyncOnly => matches!(target, ContextTarget::Async),
        }
    }
}

/// Borrowed host inputs available while one Guardian context section is built.
#[derive(Clone, Copy)]
pub struct SectionInput<'a> {
    /// Consumer for which the host is collecting context sections.
    pub target: ContextTarget,
    /// Parent conversation history available to this contribution.
    pub history: &'a dyn SectionHistory,
    /// Evidence sources and per-entry limits for this collection.
    pub transcript: &'a ConversationTranscriptConfig,
    /// Bounded root evidence resolved by the host; empty when not applicable.
    pub root_conversation: &'a [GuardianRootMessage],
    /// Bounded, role-labeled answers verified and matched to history by the host.
    pub trusted_user_answers: &'a [String],
}

/// Supplies repeatable, zero-copy access to a host-owned conversation snapshot.
///
/// Implementations return a fresh iterator for every call so independently
/// registered contributors can inspect the same history without cloning its
/// response items or taking ownership away from the host.
pub trait SectionHistory: Send + Sync {
    /// Returns borrowed response items in their original conversation order.
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_>;
}

impl SectionHistory for Vec<ResponseItem> {
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_> {
        Box::new(self.iter())
    }
}

impl<const LENGTH: usize> SectionHistory for [ResponseItem; LENGTH] {
    fn items(&self) -> Box<dyn Iterator<Item = &ResponseItem> + Send + '_> {
        Box::new(self.iter())
    }
}

/// Supplies one independently scoped section to Guardian context assembly.
///
/// Implementations declare whether they apply to synchronous review,
/// asynchronous scoring, or both. The registry filters contributors by scope
/// before invoking them. Contributors distinguish sections that do not apply
/// from required evidence that could not be collected.
/// Keep request-specific settings and history in [`SectionInput`] so the same
/// contributor can serve concurrent reviews without retaining stale state.
pub trait SectionContributor: Send + Sync {
    /// Guardian consumers that should receive this contribution.
    fn scope(&self) -> SectionScope;

    /// Builds this section using the host's current conversation snapshot.
    ///
    /// Return `Ok(None)` only when this section is optional or does not apply.
    /// Missing required evidence must return `Err`; callers must not review a
    /// partial context as though collection succeeded.
    fn contribute(&self, input: &SectionInput<'_>) -> Result<Option<ContextSection>, SectionError>;
}

/// A section could not provide the evidence needed for a valid review context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SectionError {
    /// Evidence required by this contributor for the current input is missing.
    MissingRequiredEvidence { section: &'static str },
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredEvidence { section } => {
                write!(formatter, "missing required evidence for section {section}")
            }
        }
    }
}

impl std::error::Error for SectionError {}

/// Ordered collection of independently scoped Guardian section contributors.
#[derive(Clone, Default)]
pub struct SectionRegistry {
    contributors: Vec<Arc<dyn SectionContributor>>,
}

/// Shared, process-lifetime registry of built-in Guardian sections.
///
/// Contributors store no conversation or configuration state. Each collection
/// borrows the current history and settings from [`SectionInput`], so callers
/// can reuse this registry across threads, model changes, and review targets.
pub fn default_registry() -> &'static SectionRegistry {
    static REGISTRY: LazyLock<SectionRegistry> = LazyLock::new(|| {
        let mut registry = SectionRegistry::default();
        registry.register(RootConversationSection);
        registry.register(TrustedUserAnswersSection);
        registry.register(ConversationTranscriptSection);
        registry
    });
    &REGISTRY
}

impl SectionRegistry {
    /// Adds a contributor to the end of the section collection order.
    pub fn register(&mut self, contributor: impl SectionContributor + 'static) {
        self.contributors.push(Arc::new(contributor));
    }

    /// Collects applicable sections in their original registration order.
    ///
    /// Stops at the first error without returning any partial context. The host
    /// decides whether to fall back to synchronous review or deny approval.
    pub fn collect(&self, input: &SectionInput<'_>) -> Result<Vec<ContextSection>, SectionError> {
        self.contributors
            .iter()
            .filter(|contributor| contributor.scope().includes(input.target))
            .filter_map(|contributor| contributor.contribute(input).transpose())
            .collect()
    }
}

/// Ordered evidence with a stable section identity and source-specific content.
///
/// Variants preserve provenance: transcript entries carry their original roles,
/// root messages remain line-role-labeled, and answers are host-verified fragments.
/// All currently supported sections are delivered as user-role evidence. Source
/// attribution never promotes their contents to developer instructions.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextSection {
    ConversationTranscript {
        items: Vec<ConversationTranscriptEntry>,
    },
    RootConversation {
        items: Vec<String>,
    },
    TrustedUserAnswers {
        items: Vec<String>,
    },
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod registry_tests;

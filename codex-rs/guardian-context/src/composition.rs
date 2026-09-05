//! Groups collected sections for the existing Guardian prompt layout.
//!
//! Authorization keeps its rendered fragment boundaries and registration order.
//! Transcript entries remain structured so callers retain their own rendering,
//! retention and delta policies. Composition never changes roles or budgets.

use crate::ContextSection;
use crate::ConversationTranscriptEntry;
use crate::SectionError;
use crate::SectionInput;
use crate::SectionRegistry;

/// Shared authorization prelude and transcript evidence before caller rendering.
#[derive(Debug, Default, PartialEq)]
pub struct ComposedContext {
    pub authorization: Vec<String>,
    pub transcript: Vec<ConversationTranscriptEntry>,
}

impl SectionRegistry {
    /// Collects sections and groups them without duplicating layout knowledge in callers.
    ///
    /// Like [`Self::collect`], returns no partial context if a contributor fails.
    /// Callers needing individual section identities can continue using `collect`.
    pub fn compose(&self, input: &SectionInput<'_>) -> Result<ComposedContext, SectionError> {
        let mut context = ComposedContext::default();
        for section in self.collect(input)? {
            match section {
                ContextSection::ConversationTranscript { items } => {
                    context.transcript.extend(items)
                }
                ContextSection::RootConversation { items }
                | ContextSection::TrustedUserAnswers { items } => {
                    context.authorization.extend(items)
                }
            }
        }
        Ok(context)
    }
}

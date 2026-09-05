mod reviewer_config;

use std::sync::Arc;
use std::sync::Weak;

use codex_core::StartThreadOptions;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_core::context::NodeReplReviewEvidenceMode;
use codex_extension_api::ApprovalReviewError;
use codex_extension_api::ApprovalReviewInput;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::InternalSessionSpawnFuture;
use codex_extension_api::InternalSessionSpawner;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadReadyInput;
use codex_features::Feature;
use codex_network_proxy::NetworkProxyConfig;
use codex_protocol::ThreadId;
use codex_protocol::openai_models::InputModality;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::TurnEnvironmentSelection;
use codex_protocol::user_input::UserInput;

mod prompt;

/// Guardian extension dependencies supplied by the host at construction time.
#[derive(Clone, Debug)]
pub struct GuardianExtension<S> {
    thread_manager: Weak<ThreadManager>,
    internal_session_spawner: S,
}

impl<S> GuardianExtension<S> {
    /// Creates a guardian extension with its host-owned internal-session spawner.
    pub fn new(thread_manager: Weak<ThreadManager>, internal_session_spawner: S) -> Self {
        Self {
            thread_manager,
            internal_session_spawner,
        }
    }

    /// Prepares reviewer options for the later synchronous reviewer implementation.
    pub async fn prepare_reviewer_options(
        &self,
        parent_config: &Config,
        parent_environments: &[TurnEnvironmentSelection],
        parent_model: &str,
        parent_reasoning_effort: Option<ReasoningEffort>,
        live_network_config: Option<NetworkProxyConfig>,
    ) -> Result<StartThreadOptions, ApprovalReviewError> {
        let thread_manager = self.thread_manager.upgrade().ok_or_else(|| {
            ApprovalReviewError::Failed("thread manager is no longer available".to_string())
        })?;
        reviewer_config::prepare(
            &thread_manager,
            parent_config,
            parent_environments,
            parent_model,
            parent_reasoning_effort,
            live_network_config,
        )
        .await
    }

    /// Delegates a fresh internal-session request to the host helper.
    pub fn spawn_internal_session<'a, R>(
        &'a self,
        parent_thread_id: ThreadId,
        request: R,
    ) -> InternalSessionSpawnFuture<
        'a,
        <S as InternalSessionSpawner<R>>::Spawned,
        <S as InternalSessionSpawner<R>>::Error,
    >
    where
        S: InternalSessionSpawner<R>,
    {
        self.internal_session_spawner
            .spawn_internal_session(parent_thread_id, request)
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "wired by a subsequent PR"))]
    pub(crate) async fn build_review_prompt(
        &self,
        input: &ApprovalReviewInput<'_>,
        reviewer_input_modalities: &[InputModality],
    ) -> Result<Vec<UserInput>, ApprovalReviewError> {
        let thread_manager = self.thread_manager.upgrade().ok_or_else(|| {
            ApprovalReviewError::Failed("parent thread manager is unavailable".to_string())
        })?;
        let parent = thread_manager
            .get_thread(input.thread_id)
            .await
            .map_err(|error| {
                ApprovalReviewError::Failed(format!("parent thread is unavailable: {error}"))
            })?;
        let parent_config = parent.config_snapshot().await;
        let parent_permission_profile = parent
            .restorable_thread_settings()
            .await
            .permission_profile
            .ok_or_else(|| {
                ApprovalReviewError::Failed("parent permission profile is unavailable".to_string())
            })?;
        let config = parent.config().await;
        let parent_model_info = input.thread_store.get::<ModelInfo>().ok_or_else(|| {
            ApprovalReviewError::Failed("parent model metadata is unavailable".to_string())
        })?;
        let enhanced_transcripts = config
            .features
            .enabled(Feature::GuardianEnhancedNodeReplTranscripts);
        let node_repl_evidence_mode = if parent_model_info.node_repl_auto_review_required
            || enhanced_transcripts
                && config
                    .features
                    .enabled(Feature::GuardianNodeReplTranscriptImages)
        {
            NodeReplReviewEvidenceMode::Multimodal
        } else if enhanced_transcripts {
            NodeReplReviewEvidenceMode::TextOnly
        } else {
            NodeReplReviewEvidenceMode::Disabled
        };
        let root_authorization = parent.guardian_root_snapshot().await;

        prompt::build(
            input,
            &parent_config,
            &parent_permission_profile,
            root_authorization,
            reviewer_input_modalities,
            node_repl_evidence_mode,
        )
    }
}

/// Thread-local guardian state captured after the host registers a thread.
#[derive(Clone, Debug)]
pub struct GuardianThreadContext {
    parent_thread_id: ThreadId,
}

impl<S> ThreadLifecycleContributor<Config> for GuardianExtension<S>
where
    S: Send + Sync,
{
    fn on_thread_ready<'a>(
        &'a self,
        input: ThreadReadyInput<'a, Config>,
    ) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            if input.session_source.is_internal() {
                return;
            }
            let Ok(parent_thread_id) = ThreadId::from_string(input.thread_store.level_id()) else {
                return;
            };
            let Some(thread_manager) = self.thread_manager.upgrade() else {
                return;
            };
            if thread_manager.get_thread(parent_thread_id).await.is_err() {
                return;
            }
            input
                .thread_store
                .insert(GuardianThreadContext { parent_thread_id });
        })
    }
}

/// Installs the guardian contributors into the extension registry.
pub fn install<S>(
    registry: &mut ExtensionRegistryBuilder<Config>,
    thread_manager: Weak<ThreadManager>,
    internal_session_spawner: S,
) where
    S: Send + Sync + 'static,
{
    registry.thread_lifecycle_contributor(Arc::new(GuardianExtension::new(
        thread_manager,
        internal_session_spawner,
    )));
}

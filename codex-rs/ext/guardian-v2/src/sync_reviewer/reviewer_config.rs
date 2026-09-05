use std::collections::HashMap;

use codex_core::StartThreadOptions;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_core::config::Constrained;
use codex_core::config::NetworkProxySpec;
use codex_extension_api::ApprovalReviewError;
use codex_features::Feature;
use codex_model_provider::create_model_provider;
use codex_models_manager::manager::RefreshStrategy;
use codex_network_proxy::NetworkProxyConfig;
use codex_protocol::models::BaseInstructionsProvenance;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::PermissionProfileSnapshot;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_protocol::protocol::InternalSessionSource;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::ThreadSource;
use codex_protocol::protocol::TurnEnvironmentSelection;
use tracing::warn;

/// Prepares reviewer options for the later extension-owned reviewer implementation.
/// The caller supplies the parent's effective reasoning effort and live proxy state;
/// starting the prepared internal session remains the host's responsibility.
pub(super) async fn prepare(
    thread_manager: &ThreadManager,
    parent_config: &Config,
    parent_environments: &[TurnEnvironmentSelection],
    parent_model: &str,
    parent_reasoning_effort: Option<ReasoningEffort>,
    live_network_config: Option<NetworkProxyConfig>,
) -> Result<StartThreadOptions, ApprovalReviewError> {
    let model = select_reviewer_model(
        thread_manager,
        parent_config,
        parent_model,
        parent_reasoning_effort,
    )
    .await;
    let config = isolated_reviewer_config(parent_config, parent_model, model, live_network_config)?;
    let mut environments = parent_environments.to_vec();
    for environment in &mut environments {
        match &mut environment.config {
            EnvironmentConfigState::FromThread => {}
            EnvironmentConfigState::Ready(environment_config) => {
                environment_config.permission_profile =
                    PermissionProfileSnapshot::legacy(read_only_permission_profile(
                        environment_config.permission_profile.permission_profile(),
                    ));
            }
            EnvironmentConfigState::Pending | EnvironmentConfigState::Failed(_) => {
                return Err(ApprovalReviewError::Failed(format!(
                    "guardian reviewer environment `{}` has no resolved permissions",
                    environment.environment_id
                )));
            }
        }
    }

    Ok(StartThreadOptions {
        session_source: Some(SessionSource::Internal(InternalSessionSource::Guardian)),
        thread_source: Some(ThreadSource::GuardianReview),
        environments: Some(environments),
        ..StartThreadOptions::new(config)
    })
}

struct ReviewerModel {
    info: ModelInfo,
    reasoning_effort: Option<ReasoningEffort>,
    preserve_client_developer_messages: bool,
}

async fn select_reviewer_model(
    thread_manager: &ThreadManager,
    parent_config: &Config,
    parent_model: &str,
    parent_reasoning_effort: Option<ReasoningEffort>,
) -> ReviewerModel {
    let models_manager = thread_manager.get_models_manager();
    let manager_config = parent_config.to_models_manager_config();
    let parent_model_info = models_manager
        .get_model_info(parent_model, &manager_config)
        .await;
    // TODO: Use the parent session's auth when reviewer spawning is wired;
    // resumed threads may use different auth than the thread manager.
    let provider = create_model_provider(
        parent_config.model_provider.clone(),
        Some(thread_manager.auth_manager()),
    );
    let preferred_review_model = provider.approval_review_preferred_model();
    let selected_review_model = parent_model_info
        .auto_review_model_override
        .as_deref()
        .unwrap_or(preferred_review_model);
    let available_models = models_manager
        .list_models(
            RefreshStrategy::Offline,
            parent_config.http_client_factory(),
        )
        .await;
    let review_model = available_models
        .iter()
        .find(|model| model.model == selected_review_model);
    let (model, reasoning_effort) = if let Some(review_model) = review_model {
        (
            selected_review_model.to_string(),
            preferred_reasoning_effort(
                &review_model.supported_reasoning_efforts,
                Some(review_model.default_reasoning_effort.clone()),
            ),
        )
    } else {
        (
            parent_model_info
                .auto_review_model_override
                .clone()
                .unwrap_or_else(|| parent_model.to_string()),
            preferred_reasoning_effort(
                &parent_model_info.supported_reasoning_levels,
                parent_reasoning_effort
                    .or_else(|| parent_model_info.default_reasoning_level.clone()),
            ),
        )
    };

    ReviewerModel {
        info: models_manager.get_model_info(&model, &manager_config).await,
        reasoning_effort,
        preserve_client_developer_messages: parent_model_info.node_repl_auto_review_required,
    }
}

fn preferred_reasoning_effort(
    supported_efforts: &[ReasoningEffortPreset],
    fallback: Option<ReasoningEffort>,
) -> Option<ReasoningEffort> {
    if supported_efforts
        .iter()
        .any(|effort| effort.effort == ReasoningEffort::Low)
    {
        Some(ReasoningEffort::Low)
    } else {
        fallback
    }
}

fn isolated_reviewer_config(
    parent_config: &Config,
    parent_model: &str,
    reviewer_model: ReviewerModel,
    live_network_config: Option<NetworkProxyConfig>,
) -> Result<Config, ApprovalReviewError> {
    let model_messages = reviewer_model.info.model_messages.as_ref();
    let template = model_messages
        .and_then(|messages| messages.auto_review.as_ref())
        .and_then(|messages| messages.policy_template.as_deref())
        .unwrap_or(POLICY_TEMPLATE);
    let policy = parent_config.resolve_guardian_policy(model_messages);
    let policy_prompt = template
        .trim_end()
        .replace(POLICY_PLACEHOLDER, policy.trim());

    // Preserve the parent's persistence setting so reusable reviewers can retain their rollouts.
    let mut config = parent_config.clone();
    config.model = Some(reviewer_model.info.slug.clone());
    config.model_reasoning_effort = reviewer_model.reasoning_effort;
    config.model_provider.request_max_retries = Some(1);
    config.model_provider.stream_max_retries = Some(1);

    config.base_instructions = Some(format!("{policy_prompt}\n\n{OUTPUT_CONTRACT}\n"));
    config.base_instructions_provenance = Some(BaseInstructionsProvenance::Custom);
    config.developer_instructions = None;
    config.project_doc_max_bytes = 0;
    config.personality = None;

    config.include_apps_instructions = false;
    config.include_collaboration_mode_instructions = false;
    config.include_skill_instructions = false;
    config.orchestrator_skills_enabled = false;
    config.orchestrator_mcp_enabled = false;
    config.agents_enabled = false;
    config.memories.use_memories = false;
    config.memories.dedicated_tools = false;

    config.notify = None;
    config.token_budget = None;
    config.rollout_budget = None;
    config.max_goal_token_budget = None;

    config.permissions.approval_policy = Constrained::allow_only(AskForApproval::Never);

    let permission_profile =
        read_only_permission_profile(parent_config.permissions.permission_profile());
    config
        .permissions
        .set_permission_profile(permission_profile)
        .map_err(|error| {
            ApprovalReviewError::Failed(format!(
                "guardian reviewer could not set read-only permissions: {error}"
            ))
        })?;
    config.mcp_servers.set(HashMap::new()).map_err(|error| {
        ApprovalReviewError::Failed(format!(
            "guardian reviewer could not clear MCP servers: {error}"
        ))
    })?;

    if let Some(live_network_config) = live_network_config
        && config.permissions.network.is_some()
    {
        let network_constraints = config
            .config_layer_stack
            .requirements()
            .network
            .as_ref()
            .map(|network| network.value.clone());
        config.permissions.network = Some(
            NetworkProxySpec::from_config_and_constraints(
                live_network_config,
                network_constraints,
                config.permissions.permission_profile(),
            )
            .map_err(|error| {
                ApprovalReviewError::Failed(format!(
                    "guardian reviewer could not preserve managed network restrictions: {error}"
                ))
            })?,
        );
    }

    for feature in DISABLED_FEATURES {
        config.features.disable(feature).map_err(|error| {
            ApprovalReviewError::Failed(format!(
                "guardian reviewer could not disable `features.{}`: {error}",
                feature.key()
            ))
        })?;
        if config.features.enabled(feature) {
            warn!(
                feature = feature.key(),
                "managed requirements keep a Guardian reviewer feature enabled"
            );
        }
    }

    if reviewer_model.preserve_client_developer_messages {
        config
            .features
            .enable(Feature::RetainClientDeveloperMessages)
            .map_err(|error| {
                ApprovalReviewError::Failed(format!(
                    "guardian reviewer could not preserve Code Mode developer policy: {error}"
                ))
            })?;
    }

    if reviewer_model.info.slug != parent_model {
        config.model_context_window = None;
        config.model_auto_compact_token_limit = None;
    }

    Ok(config)
}

fn read_only_permission_profile(permission_profile: &PermissionProfile) -> PermissionProfile {
    permission_profile
        .intersect_with_read_only()
        .unwrap_or(PermissionProfile::External {
            network: NetworkSandboxPolicy::Restricted,
        })
}

const POLICY_TEMPLATE: &str = include_str!("policy_template.md");
const POLICY_PLACEHOLDER: &str = "{{ tenant_policy_config }}";
const OUTPUT_CONTRACT: &str = r#"You may use read-only tool checks to gather any additional context you need before deciding. When you are ready to answer, your final message must be strict JSON.

For low-risk actions, give the final answer directly: {"outcome":"allow"}.

For anything else, use this JSON schema:
{
  "risk_level": "low" | "medium" | "high" | "critical",
  "user_authorization": "unknown" | "low" | "medium" | "high",
  "outcome": "allow" | "deny",
  "rationale": string
}"#;

const DISABLED_FEATURES: [Feature; 20] = [
    Feature::Apps,
    Feature::CodexHooks,
    Feature::Collab,
    Feature::CurrentTimeReminder,
    Feature::EnableMcpApps,
    Feature::Goals,
    Feature::GuardianApproval,
    Feature::GuardianExt,
    Feature::GuardianV2,
    Feature::MemoryTool,
    Feature::MultiAgentV2,
    Feature::Personality,
    Feature::Plugins,
    Feature::RecommendedPlugins,
    Feature::RolloutBudget,
    Feature::StandaloneWebSearch,
    Feature::TokenBudget,
    Feature::ToolSuggest,
    Feature::WebSearchCached,
    Feature::WebSearchRequest,
];

#[cfg(test)]
#[path = "reviewer_config_tests.rs"]
mod tests;

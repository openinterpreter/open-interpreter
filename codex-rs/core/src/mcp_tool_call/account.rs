//! Resolves account identity before native Apps approvals. Required selectors
//! must be valid; optional catalog identities never block legacy calls.

use super::MCP_TOOL_LINK_ID_META_KEY;
use codex_mcp::MCP_TOOL_CODEX_APPS_META_KEY;
use codex_mcp::ToolInfo;
use serde_json::Value as JsonValue;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum McpToolAccountError {
    #[error("This app tool requires a non-empty string link_id argument")]
    InvalidSelector,
}

pub(super) fn resolve_account(
    tool_info: &ToolInfo,
    arguments: Option<&JsonValue>,
) -> Result<Option<String>, McpToolAccountError> {
    let tool_meta = tool_info.tool.meta.as_deref();
    let requires_explicit_link_id = tool_meta
        .and_then(|meta| meta.get(MCP_TOOL_CODEX_APPS_META_KEY))
        .and_then(|meta| meta.get("requires_explicit_link_id"))
        .and_then(JsonValue::as_bool)
        == Some(true);
    let link_id = if requires_explicit_link_id {
        arguments.and_then(|arguments| arguments.get(MCP_TOOL_LINK_ID_META_KEY))
    } else {
        tool_meta.and_then(|meta| meta.get(MCP_TOOL_LINK_ID_META_KEY))
    }
    .and_then(JsonValue::as_str)
    .filter(|link_id| !link_id.trim().is_empty())
    .map(str::to_owned);

    if requires_explicit_link_id && link_id.is_none() {
        Err(McpToolAccountError::InvalidSelector)
    } else {
        Ok(link_id)
    }
}

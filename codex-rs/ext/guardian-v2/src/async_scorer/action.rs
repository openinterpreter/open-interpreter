//! Renders bounded planned-action JSON while preserving tool and call identity.
//! String truncation precedes omission of optional fields.

use codex_extension_api::ToolName;
use codex_extension_api::ToolPayload;
use codex_protocol::protocol::TruncationPolicy;
use serde_json::json;

use super::transcript::truncate_entry;

pub(super) struct GuardianAction {
    pub(super) tool_name: ToolName,
    pub(super) payload: ToolPayload,
}

pub(super) struct RenderedAction {
    pub(super) text: String,
    pub(super) original_bytes: usize,
}

impl GuardianAction {
    pub(super) fn render(self, max_action_tokens: usize) -> serde_json::Result<RenderedAction> {
        let arguments = match self.payload {
            ToolPayload::Function { arguments } => {
                serde_json::from_str(&arguments).unwrap_or(serde_json::Value::String(arguments))
            }
            ToolPayload::Custom { input } => serde_json::Value::String(input),
            ToolPayload::ToolSearch { arguments } => json!(arguments),
        };
        let mut action = match arguments {
            serde_json::Value::Object(arguments) => arguments,
            arguments => serde_json::Map::from_iter([("arguments".to_owned(), arguments)]),
        };
        action.insert(
            "tool".to_owned(),
            serde_json::Value::String(self.tool_name.to_string()),
        );

        action.sort_keys();
        action
            .values_mut()
            .for_each(serde_json::Value::sort_all_objects);
        let max_action_bytes = TruncationPolicy::Tokens(max_action_tokens).byte_budget();
        let rendered = serde_json::to_string_pretty(&action)?;
        let original_bytes = rendered.len();
        if rendered.len().saturating_add(1) <= max_action_bytes {
            return Ok(RenderedAction {
                text: rendered,
                original_bytes,
            });
        }

        if let Some(rendered) = fit_action_to_budget(&action, max_action_bytes, max_action_tokens)?
        {
            return Ok(RenderedAction {
                text: rendered,
                original_bytes,
            });
        }

        let mut omission_key = "_guardian_omitted_fields".to_owned();
        while action.contains_key(&omission_key) {
            omission_key.push('_');
        }
        let mut retained = serde_json::Map::new();
        for key in ["tool", "call_id"] {
            if let Some(value) = action.get(key) {
                retained.insert(key.to_owned(), value.clone());
            }
        }
        let mut omitted = action.len().saturating_sub(retained.len());
        retained.insert(omission_key.clone(), json!(omitted));

        let mut optional_fields = action
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "tool" | "call_id"))
            .collect::<Vec<_>>();
        optional_fields.sort_by_key(|(key, _)| {
            !matches!(
                key.as_str(),
                "arguments" | "cmd" | "command" | "input" | "patch" | "path" | "url"
            )
        });
        for (key, value) in optional_fields {
            let mut candidate = retained.clone();
            candidate.insert(key.clone(), value.clone());
            candidate.insert(omission_key.clone(), json!(omitted.saturating_sub(1)));
            candidate.sort_keys();
            let minimized = render_action_with_limit(&candidate, /*max_tokens*/ 0)?;
            if minimized.len().saturating_add(1) <= max_action_bytes {
                retained = candidate;
                omitted = omitted.saturating_sub(1);
            }
        }

        retained.sort_keys();
        let rendered = fit_action_to_budget(&retained, max_action_bytes, max_action_tokens)?
            .ok_or_else(|| {
                serde_json::Error::io(std::io::Error::other(format!(
                    "Guardian action identity exceeds the {max_action_tokens}-token limit"
                )))
            })?;
        Ok(RenderedAction {
            text: rendered,
            original_bytes,
        })
    }
}

fn fit_action_to_budget(
    action: &serde_json::Map<String, serde_json::Value>,
    max_action_bytes: usize,
    max_action_tokens: usize,
) -> serde_json::Result<Option<String>> {
    let mut low = 0usize;
    let mut high = max_action_tokens.saturating_add(1);
    let mut best = None;

    while low < high {
        let max_tokens = low + (high - low) / 2;
        let rendered = render_action_with_limit(action, max_tokens)?;
        if rendered.len().saturating_add(1) <= max_action_bytes {
            best = Some(rendered);
            low = max_tokens.saturating_add(1);
        } else {
            high = max_tokens;
        }
    }

    Ok(best)
}

fn render_action_with_limit(
    action: &serde_json::Map<String, serde_json::Value>,
    max_tokens: usize,
) -> serde_json::Result<String> {
    let mut truncated = action.clone();
    for (key, value) in &mut truncated {
        if !matches!(key.as_str(), "tool" | "call_id") {
            truncate_action_value(value, max_tokens);
        }
    }
    serde_json::to_string_pretty(&truncated)
}

fn truncate_action_value(value: &mut serde_json::Value, max_tokens: usize) {
    match value {
        serde_json::Value::String(text) => {
            let truncated = truncate_entry(text, max_tokens);
            if truncated.len() < text.len() {
                *text = truncated;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                truncate_action_value(value, max_tokens);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                truncate_action_value(value, max_tokens);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;

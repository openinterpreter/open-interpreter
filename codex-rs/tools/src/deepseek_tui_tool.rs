use crate::ResponsesApiTool;
use crate::ToolSpec;
use crate::parse_tool_input_schema;
use serde::Deserialize;

#[derive(Deserialize)]
struct DeepSeekTuiToolFixture {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

pub fn create_deepseek_tui_tools() -> Vec<ToolSpec> {
    let tools: Vec<DeepSeekTuiToolFixture> =
        match serde_json::from_str(include_str!("deepseek_tui_tools.json")) {
            Ok(tools) => tools,
            Err(err) => panic!("deepseek-tui tool schema JSON must be valid: {err}"),
        };
    tools
        .into_iter()
        .map(|tool| {
            let parameters = match parse_tool_input_schema(&tool.parameters) {
                Ok(parameters) => parameters,
                Err(err) => {
                    panic!("deepseek-tui tool parameters must be valid JSON schema: {err}")
                }
            };
            ToolSpec::Function(ResponsesApiTool {
                name: tool.name,
                description: tool.description,
                strict: false,
                defer_loading: None,
                parameters,
                output_schema: None,
            })
        })
        .collect()
}

pub fn create_deepseek_tui_chat_tools_json() -> Vec<serde_json::Value> {
    let tools: Vec<DeepSeekTuiToolFixture> =
        match serde_json::from_str(include_str!("deepseek_tui_tools.json")) {
            Ok(tools) => tools,
            Err(err) => panic!("deepseek-tui tool schema JSON must be valid: {err}"),
        };
    tools
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn deepseek_tui_tool_surface_has_expected_count_and_order() {
        let tools = create_deepseek_tui_tools();

        assert_eq!(tools.len(), 84);
        assert_eq!(tools.first().map(ToolSpec::name), Some("agent_close"));
        assert_eq!(
            tools.last().map(ToolSpec::name),
            Some("tool_search_tool_bm25")
        );
    }
}

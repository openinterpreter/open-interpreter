use codex_tools::AdditionalProperties;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;

#[derive(Clone, Copy)]
pub enum KimiCodeCronHandler {
    Create,
    Delete,
    List,
}

impl ToolExecutor<ToolInvocation> for KimiCodeCronHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(match self {
            Self::Create => "CronCreate",
            Self::Delete => "CronDelete",
            Self::List => "CronList",
        })
    }

    fn spec(&self) -> ToolSpec {
        let name = self.tool_name().name;
        ToolSpec::Function(ResponsesApiTool {
            name: name.clone(),
            description: format!("Open Interpreter Kimi Code compatibility handler for {name}."),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                Default::default(),
                /*required*/ None,
                Some(AdditionalProperties::from(true)),
            ),
            output_schema: None,
        })
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        false
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move {
            let output = match self {
                Self::Create => {
                    let args: CronCreateArgs = parse_invocation_arguments(&invocation)?;
                    invocation
                        .session
                        .services
                        .kimi_cron
                        .create(&args.cron, args.prompt, args.recurring.unwrap_or(true))
                        .await
                }
                Self::Delete => {
                    let args: CronDeleteArgs = parse_invocation_arguments(&invocation)?;
                    invocation.session.services.kimi_cron.delete(&args.id).await
                }
                Self::List => {
                    let _: CronListArgs = parse_invocation_arguments(&invocation)?;
                    Ok(invocation.session.services.kimi_cron.list().await)
                }
            }
            .map_err(FunctionCallError::RespondToModel)?;
            Ok(
                boxed_tool_output(FunctionToolOutput::from_text(output, Some(true)))
                    as Box<dyn ToolOutput>,
            )
        })
    }
}

impl CoreToolRuntime for KimiCodeCronHandler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Deserialize)]
struct CronCreateArgs {
    cron: String,
    prompt: String,
    recurring: Option<bool>,
}

#[derive(Deserialize)]
struct CronDeleteArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CronListArgs {}

fn parse_invocation_arguments<T>(invocation: &ToolInvocation) -> Result<T, FunctionCallError>
where
    T: for<'de> Deserialize<'de>,
{
    let ToolPayload::Function { arguments } = &invocation.payload else {
        return Err(FunctionCallError::RespondToModel(
            "Kimi Code cron tool received unsupported tool payload".to_string(),
        ));
    };
    parse_arguments(arguments)
}

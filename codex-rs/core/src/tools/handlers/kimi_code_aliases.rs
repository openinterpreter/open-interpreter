use std::sync::Mutex;
use std::time::Instant;

use codex_tools::AdditionalProperties;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;
use serde::Serialize;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::kimi_code_format::GoalBudgetUnit;
use crate::tools::handlers::kimi_code_format::KimiTodo;
use crate::tools::handlers::kimi_code_format::format_budget;
use crate::tools::handlers::kimi_code_format::format_elapsed;
use crate::tools::handlers::kimi_code_format::format_tokens;
use crate::tools::handlers::kimi_code_format::render_todo_list;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;

const TODO_LIST_WRITE_REMINDER: &str = "Ensure that you continue to use the todo list to track progress. Mark tasks done immediately after finishing them, and keep exactly one task in_progress when work is underway.";

#[derive(Clone, Copy)]
pub enum KimiCodeAliasHandler {
    CreateGoal,
    GetGoal,
    SetGoalBudget,
    TodoList,
    UpdateGoal,
}

impl ToolExecutor<ToolInvocation> for KimiCodeAliasHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(match self {
            Self::CreateGoal => "CreateGoal",
            Self::GetGoal => "GetGoal",
            Self::SetGoalBudget => "SetGoalBudget",
            Self::TodoList => "TodoList",
            Self::UpdateGoal => "UpdateGoal",
        })
    }

    fn spec(&self) -> ToolSpec {
        let name = self.tool_name().name;
        ToolSpec::Function(ResponsesApiTool {
            name: name.clone(),
            description: format!("Open Interpreter Kimi Code compatibility alias for {name}."),
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
            match self {
                Self::CreateGoal => handle_create_goal(invocation).await,
                Self::GetGoal => handle_get_goal(invocation).await,
                Self::SetGoalBudget => handle_set_goal_budget(invocation).await,
                Self::TodoList => handle_todo_list(invocation).await,
                Self::UpdateGoal => handle_update_goal(invocation).await,
            }
        })
    }
}

impl CoreToolRuntime for KimiCodeAliasHandler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Default)]
struct KimiCodeState {
    goal: Option<KimiGoal>,
    todos: Vec<KimiTodo>,
}

struct KimiGoal {
    objective: String,
    completion_criterion: Option<String>,
    started_at: Instant,
    output_tokens_at_start: i64,
    turn_budget: Option<u64>,
    token_budget: Option<u64>,
    wall_clock_budget_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateGoalArgs {
    objective: String,
    completion_criterion: Option<String>,
    #[serde(default)]
    replace: bool,
}

async fn handle_create_goal(
    invocation: ToolInvocation,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let args: CreateGoalArgs = parse_invocation_arguments(&invocation)?;
    let objective = args.objective.trim().to_string();
    if objective.is_empty() {
        return model_error("Goal not created: objective must not be empty.");
    }
    let output_tokens_at_start = current_output_tokens(&invocation).await;
    let state = kimi_state(&invocation);
    if lock_state(&state).goal.is_some() && !args.replace {
        return model_error(
            "Goal not created: a current goal already exists. Pass replace=true to replace it.",
        );
    }
    if let Some(state_db) = invocation.session.state_db() {
        state_db
            .thread_goals()
            .replace_thread_goal(
                invocation.session.thread_id(),
                &objective,
                codex_state::ThreadGoalStatus::Active,
                /*token_budget*/ None,
            )
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!("failed to persist goal: {err}"))
            })?;
    }
    lock_state(&state).goal = Some(KimiGoal {
        objective: objective.clone(),
        completion_criterion: args
            .completion_criterion
            .map(|criterion| criterion.trim().chars().take(4_000).collect()),
        started_at: Instant::now(),
        output_tokens_at_start,
        turn_budget: None,
        token_budget: None,
        wall_clock_budget_ms: None,
    });
    goal_json_output(&invocation, Some(&state)).await
}

async fn handle_get_goal(
    invocation: ToolInvocation,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let _: serde_json::Value = parse_invocation_arguments(&invocation)?;
    let state = kimi_state(&invocation);
    goal_json_output(&invocation, Some(&state)).await
}

#[derive(Deserialize)]
struct SetGoalBudgetArgs {
    value: f64,
    unit: GoalBudgetUnit,
}

async fn handle_set_goal_budget(
    invocation: ToolInvocation,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let args: SetGoalBudgetArgs = parse_invocation_arguments(&invocation)?;
    if !args.value.is_finite() || args.value <= 0.0 {
        return model_error("Goal budget not set: value must be positive.");
    }
    let normalized_value = match args.unit {
        GoalBudgetUnit::Turns | GoalBudgetUnit::Tokens => args.value.round().max(1.0),
        GoalBudgetUnit::Milliseconds
        | GoalBudgetUnit::Seconds
        | GoalBudgetUnit::Minutes
        | GoalBudgetUnit::Hours => args.value,
    };
    let state = kimi_state(&invocation);
    let token_budget = {
        let mut state = lock_state(&state);
        let Some(goal) = state.goal.as_mut() else {
            return text_output("Goal budget not set: no current goal.");
        };
        match args.unit {
            GoalBudgetUnit::Turns => goal.turn_budget = Some(normalized_value as u64),
            GoalBudgetUnit::Tokens => goal.token_budget = Some(normalized_value as u64),
            GoalBudgetUnit::Milliseconds
            | GoalBudgetUnit::Seconds
            | GoalBudgetUnit::Minutes
            | GoalBudgetUnit::Hours => {
                let multiplier = match args.unit {
                    GoalBudgetUnit::Milliseconds => 1.0,
                    GoalBudgetUnit::Seconds => 1_000.0,
                    GoalBudgetUnit::Minutes => 60_000.0,
                    GoalBudgetUnit::Hours => 3_600_000.0,
                    GoalBudgetUnit::Turns | GoalBudgetUnit::Tokens => unreachable!(),
                };
                let milliseconds = (normalized_value * multiplier).round() as u64;
                if !(1_000..=86_400_000).contains(&milliseconds) {
                    return text_output(format!(
                        "Goal budget not set: {} is not a reasonable goal budget.",
                        format_budget(normalized_value, args.unit)
                    ));
                }
                goal.wall_clock_budget_ms = Some(milliseconds);
            }
        }
        goal.token_budget
    };
    if let Some(state_db) = invocation.session.state_db()
        && let Some(token_budget) = token_budget
    {
        state_db
            .thread_goals()
            .update_thread_goal(
                invocation.session.thread_id(),
                codex_state::GoalUpdate {
                    objective: None,
                    status: None,
                    token_budget: Some(Some(i64::try_from(token_budget).unwrap_or(i64::MAX))),
                    expected_goal_id: None,
                },
            )
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!("failed to persist goal budget: {err}"))
            })?;
    }
    text_output(format!(
        "Goal budget set: {}.",
        format_budget(normalized_value, args.unit)
    ))
}

#[derive(Deserialize)]
struct TodoListArgs {
    todos: Option<Vec<KimiTodo>>,
}

async fn handle_todo_list(
    invocation: ToolInvocation,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let args: TodoListArgs = parse_invocation_arguments(&invocation)?;
    let state = kimi_state(&invocation);
    let output = {
        let mut state = lock_state(&state);
        match args.todos {
            None => render_todo_list(&state.todos),
            Some(todos) if todos.is_empty() => {
                state.todos.clear();
                "Todo list cleared.".to_string()
            }
            Some(todos) => {
                state.todos = todos;
                format!(
                    "Todo list updated.\n{}\n\n{TODO_LIST_WRITE_REMINDER}",
                    render_todo_list(&state.todos)
                )
            }
        }
    };
    text_output(output)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GoalStatus {
    Active,
    Complete,
    Blocked,
}

#[derive(Deserialize)]
struct UpdateGoalArgs {
    status: GoalStatus,
}

async fn handle_update_goal(
    invocation: ToolInvocation,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let args: UpdateGoalArgs = parse_invocation_arguments(&invocation)?;
    let current_tokens = current_output_tokens(&invocation).await;
    let state = kimi_state(&invocation);
    let output = {
        let mut state = lock_state(&state);
        match args.status {
            GoalStatus::Active => {
                if state.goal.is_some() {
                    "Goal resumed.".to_string()
                } else {
                    "Goal not resumed: no current goal.".to_string()
                }
            }
            GoalStatus::Complete | GoalStatus::Blocked => {
                let Some(goal) = state.goal.take() else {
                    return text_output(match args.status {
                        GoalStatus::Complete => "Goal not completed: no active goal.",
                        GoalStatus::Blocked => "Goal not blocked: no active goal.",
                        GoalStatus::Active => unreachable!(),
                    });
                };
                let elapsed_ms = goal_elapsed_ms(&goal);
                let tokens = current_tokens.saturating_sub(goal.output_tokens_at_start);
                let stats = format!(
                    "Worked 0 turns over {}, using {} tokens.",
                    format_elapsed(elapsed_ms),
                    format_tokens(tokens)
                );
                match args.status {
                    GoalStatus::Complete => format!(
                        "Goal completed successfully.\n{stats}\n\nWrite a concise final message for the user. State that the goal is complete, summarize the main work completed, and mention any validation you ran. Do not call more goal tools."
                    ),
                    GoalStatus::Blocked => format!(
                        "Goal blocked.\n{stats}\n\nWrite a concise final message for the user. State that the goal is blocked, explain the concrete blocker, and say what input or change is needed before work can continue. Do not call more goal tools."
                    ),
                    GoalStatus::Active => unreachable!(),
                }
            }
        }
    };
    if let Some(state_db) = invocation.session.state_db() {
        match args.status {
            GoalStatus::Active => {
                state_db
                    .thread_goals()
                    .update_thread_goal(
                        invocation.session.thread_id(),
                        codex_state::GoalUpdate {
                            objective: None,
                            status: Some(codex_state::ThreadGoalStatus::Active),
                            token_budget: None,
                            expected_goal_id: None,
                        },
                    )
                    .await
            }
            GoalStatus::Complete | GoalStatus::Blocked => state_db
                .thread_goals()
                .delete_thread_goal(invocation.session.thread_id())
                .await
                .map(|_| None),
        }
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to persist goal status: {err}"))
        })?;
    }
    text_output(output)
}

fn kimi_state(invocation: &ToolInvocation) -> std::sync::Arc<Mutex<KimiCodeState>> {
    invocation
        .session
        .services
        .thread_extension_data
        .get_or_init(|| Mutex::new(KimiCodeState::default()))
}

fn lock_state(state: &Mutex<KimiCodeState>) -> std::sync::MutexGuard<'_, KimiCodeState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn parse_invocation_arguments<T>(invocation: &ToolInvocation) -> Result<T, FunctionCallError>
where
    T: for<'de> Deserialize<'de>,
{
    let ToolPayload::Function { arguments } = &invocation.payload else {
        return Err(FunctionCallError::RespondToModel(
            "Kimi Code alias received unsupported tool payload".to_string(),
        ));
    };
    parse_arguments(arguments)
}

async fn current_output_tokens(invocation: &ToolInvocation) -> i64 {
    invocation
        .session
        .total_token_usage()
        .await
        .map(|usage| usage.output_tokens)
        .unwrap_or_default()
}

async fn goal_json_output(
    invocation: &ToolInvocation,
    state: Option<&Mutex<KimiCodeState>>,
) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    let current_tokens = current_output_tokens(invocation).await;
    let goal = state.and_then(|state| {
        lock_state(state)
            .goal
            .as_ref()
            .map(|goal| goal_snapshot(goal, current_tokens))
    });
    let output = serde_json::to_string_pretty(&GoalResult { goal }).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to serialize Kimi goal: {err}"))
    })?;
    text_output(output)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalSnapshot {
    objective: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion_criterion: Option<String>,
    status: &'static str,
    turns_used: u64,
    tokens_used: i64,
    wall_clock_ms: u64,
    budget: GoalBudgetSnapshot,
}

#[derive(Serialize)]
struct GoalResult {
    goal: Option<GoalSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalBudgetSnapshot {
    token_budget: Option<u64>,
    turn_budget: Option<u64>,
    wall_clock_budget_ms: Option<u64>,
    remaining_tokens: Option<u64>,
    remaining_turns: Option<u64>,
    remaining_wall_clock_ms: Option<u64>,
    token_budget_reached: bool,
    turn_budget_reached: bool,
    wall_clock_budget_reached: bool,
    over_budget: bool,
}

fn goal_snapshot(goal: &KimiGoal, current_tokens: i64) -> GoalSnapshot {
    let tokens_used = current_tokens.saturating_sub(goal.output_tokens_at_start);
    let tokens_used_u64 = u64::try_from(tokens_used).unwrap_or_default();
    let wall_clock_ms = goal_elapsed_ms(goal);
    let remaining_tokens = goal
        .token_budget
        .map(|budget| budget.saturating_sub(tokens_used_u64));
    let remaining_turns = goal.turn_budget;
    let remaining_wall_clock_ms = goal
        .wall_clock_budget_ms
        .map(|budget| budget.saturating_sub(wall_clock_ms));
    let token_budget_reached = goal
        .token_budget
        .is_some_and(|budget| tokens_used_u64 >= budget);
    let turn_budget_reached = goal.turn_budget.is_some_and(|budget| budget == 0);
    let wall_clock_budget_reached = goal
        .wall_clock_budget_ms
        .is_some_and(|budget| wall_clock_ms >= budget);
    GoalSnapshot {
        objective: goal.objective.clone(),
        completion_criterion: goal.completion_criterion.clone(),
        status: "active",
        turns_used: 0,
        tokens_used,
        wall_clock_ms,
        budget: GoalBudgetSnapshot {
            token_budget: goal.token_budget,
            turn_budget: goal.turn_budget,
            wall_clock_budget_ms: goal.wall_clock_budget_ms,
            remaining_tokens,
            remaining_turns,
            remaining_wall_clock_ms,
            token_budget_reached,
            turn_budget_reached,
            wall_clock_budget_reached,
            over_budget: token_budget_reached || turn_budget_reached || wall_clock_budget_reached,
        },
    }
}

fn goal_elapsed_ms(goal: &KimiGoal) -> u64 {
    u64::try_from(goal.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn text_output(text: impl Into<String>) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
    Ok(boxed_tool_output(FunctionToolOutput::from_text(
        text.into(),
        Some(true),
    )))
}

fn model_error<T>(message: impl Into<String>) -> Result<T, FunctionCallError> {
    Err(FunctionCallError::RespondToModel(message.into()))
}

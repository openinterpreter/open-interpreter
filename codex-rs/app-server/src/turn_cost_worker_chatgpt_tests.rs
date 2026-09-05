//! Checks unusable SiWC estimates and exact conversion; public app-server/OTLP coverage lives in turn_cost_otel.

use super::*;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use test_case::test_case;

#[test_case(json!(null), json!(["resp-one", "resp-two"]), None; "hidden or incomplete amount")]
#[test_case(json!(-1), json!(["resp-one", "resp-two"]), None; "negative amount")]
#[test_case(json!(1), json!(null), None; "missing settlement")]
#[test_case(json!(1), json!([]), None; "unsettled")]
#[test_case(json!(1), json!(["unrelated"]), None; "wrong response")]
#[test_case(json!(1), json!(["resp-one"]), None; "only one response settled")]
#[test_case(json!(0), json!(["resp-one", "resp-two"]), Some("0.000000"); "zero")]
#[test_case(json!(i64::MAX), json!(["resp-one", "resp-two"]), Some("9223372036854.775807"); "integer precision")]
#[tokio::test]
async fn chatgpt_cost_requires_visible_amount_and_matching_settlement(
    micros: Value,
    settled_ids: Value,
    expected: Option<&str>,
) {
    let server = MockServer::start().await;
    let mut runtime = test_runtime(
        &server,
        AuthManager::from_auth_for_testing(CodexAuth::create_dummy_chatgpt_auth_for_testing()),
    )
    .await;
    let thread_id = ThreadId::new();
    runtime.turns.insert(
        "turn-1".to_string(),
        TurnCostEntry {
            thread_id,
            session_telemetry: test_session_telemetry(thread_id),
            expected_response_ids: HashSet::from(["resp-one".to_string(), "resp-two".to_string()]),
            status: TurnCostStatus::Completed,
            next_poll_at: Instant::now(),
            attempt_count: 0,
        },
    );
    Mock::given(method("POST"))
        .and(path("/api/codex/usage/thread-estimates/query"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"threads": [{
                "thread_id": thread_id, "turns": [{"turn_id": "turn-1",
                    "estimated_usage_usd_micros": micros, "settled_response_ids": settled_ids}]
            }]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let costs = runtime
        .query_turn_costs(&["turn-1".to_string()])
        .await
        .expect("query")
        .expect("SiWC enabled");
    assert_eq!(
        costs,
        expected
            .map(|amount| ApiKeyTurnCost {
                turn_id: "turn-1".to_string(),
                status: ApiKeyTurnCostStatus::Priced,
                total_usd: Some(amount.to_string()),
                event_count: Some(2),
                responses: None,
                model: None,
                speed: None,
                reasoning_effort: None,
            })
            .into_iter()
            .collect::<Vec<_>>()
    );
}

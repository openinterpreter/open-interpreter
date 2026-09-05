//! Regression coverage for Guardian's truncation marker and UTF-8 boundaries.

use codex_protocol::protocol::TruncationPolicy;
use pretty_assertions::assert_eq;

use super::truncate_text;

#[test]
fn truncation_preserves_prefix_suffix_and_utf8_boundaries() {
    let text = format!("start {} end", "é🙂".repeat(/*n*/ 2_000));
    let truncated = truncate_text(&text, /*max_tokens*/ 200);
    let omitted_tokens = TruncationPolicy::Bytes(text.len() - 800).token_budget();
    let marker = format!("<truncated omitted_approx_tokens=\"{omitted_tokens}\" />");
    assert!(truncated.starts_with("start "));
    assert!(truncated.ends_with(" end"));
    assert!(truncated.contains(&marker));
    assert!(truncated.len() <= 800);

    assert_eq!(truncate_text("é🙂", /*max_tokens*/ 2), "é🙂");
    assert_eq!(truncate_text("", /*max_tokens*/ 0), "");
    assert_eq!(
        truncate_text("é🙂", /*max_tokens*/ 0),
        "<truncated omitted_approx_tokens=\"2\" />"
    );
    assert_eq!(
        truncate_text("é🙂", /*max_tokens*/ 1),
        "<truncated omitted_approx_tokens=\"1\" />"
    );
}

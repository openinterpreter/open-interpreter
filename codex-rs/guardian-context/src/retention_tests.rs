use super::*;
use pretty_assertions::assert_eq;

#[test]
fn user_selection_preserves_anchors_and_fills_from_newest() {
    let messages = [
        UserMessageCost {
            index: 2,
            tokens: 4,
        },
        UserMessageCost {
            index: 5,
            tokens: 3,
        },
        UserMessageCost {
            index: 8,
            tokens: 6,
        },
        UserMessageCost {
            index: 10,
            tokens: 2,
        },
    ];
    for (budget, indices, tokens) in [
        (0, vec![2], 4),
        (4, vec![2], 4),
        (6, vec![2, 10], 6),
        (9, vec![2, 10, 5], 9),
        (12, vec![2, 10, 8], 12),
        (15, vec![2, 10, 8, 5], 15),
    ] {
        assert_eq!(
            select_user_messages(&messages, budget),
            UserMessageSelection { indices, tokens },
        );
    }
    assert_eq!(
        select_user_messages(&[], /*max_message_tokens*/ 10),
        UserMessageSelection::default(),
    );
    assert_eq!(
        select_user_messages(&messages[..1], /*max_message_tokens*/ 10),
        UserMessageSelection {
            indices: vec![2],
            tokens: 4
        },
    );
}

use crate::ids::RequestId;
use crate::telegram::envelope::{ChatAvailableReactions, ReactionType};
use crate::telegram::requests::{
    get_suitable_discussion_chats, set_chat_available_reactions, set_chat_discussion_group,
    toggle_chat_has_protected_content, toggle_supergroup_has_hidden_members,
    toggle_supergroup_is_all_history_available, toggle_supergroup_is_forum,
    toggle_supergroup_join_to_send_messages, upgrade_basic_group_chat_to_supergroup_chat,
};
use serde_json::Value;

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn forum_toggle_sends_tabs_only_when_enabling() {
    let on = parse(&toggle_supergroup_is_forum(RequestId(1), 42, true, true));
    assert_eq!(on["@type"], "toggleSupergroupIsForum");
    assert_eq!(on["supergroup_id"], 42);
    assert_eq!(on["is_forum"], true);
    assert_eq!(on["has_forum_tabs"], true);
    let off = parse(&toggle_supergroup_is_forum(RequestId(2), 42, false, true));
    assert_eq!(off["is_forum"], false);
    assert_eq!(off["has_forum_tabs"], false);
}

#[test]
fn supergroup_toggles_match_the_1_8_67_schema() {
    let history = parse(&toggle_supergroup_is_all_history_available(
        RequestId(3),
        7,
        true,
    ));
    assert_eq!(history["@type"], "toggleSupergroupIsAllHistoryAvailable");
    assert_eq!(history["is_all_history_available"], true);
    let join = parse(&toggle_supergroup_join_to_send_messages(
        RequestId(4),
        7,
        false,
    ));
    assert_eq!(join["@type"], "toggleSupergroupJoinToSendMessages");
    assert_eq!(join["join_to_send_messages"], false);
    let hidden = parse(&toggle_supergroup_has_hidden_members(RequestId(5), 7, true));
    assert_eq!(hidden["@type"], "toggleSupergroupHasHiddenMembers");
    assert_eq!(hidden["has_hidden_members"], true);
    assert_eq!(hidden["supergroup_id"], 7);
}

#[test]
fn protected_content_and_upgrade_target_the_chat() {
    let protected = parse(&toggle_chat_has_protected_content(
        RequestId(6),
        -100123,
        true,
    ));
    assert_eq!(protected["@type"], "toggleChatHasProtectedContent");
    assert_eq!(protected["chat_id"], -100123);
    assert_eq!(protected["has_protected_content"], true);
    let upgrade = parse(&upgrade_basic_group_chat_to_supergroup_chat(
        RequestId(7),
        -555,
    ));
    assert_eq!(upgrade["@type"], "upgradeBasicGroupChatToSupergroupChat");
    assert_eq!(upgrade["chat_id"], -555);
}

#[test]
fn discussion_requests_cover_both_sides() {
    let list = parse(&get_suitable_discussion_chats(RequestId(8)));
    assert_eq!(list["@type"], "getSuitableDiscussionChats");
    let link = parse(&set_chat_discussion_group(RequestId(9), -100, -200));
    assert_eq!(link["@type"], "setChatDiscussionGroup");
    assert_eq!(link["chat_id"], -100);
    assert_eq!(link["discussion_chat_id"], -200);
    // Unlinking from the group side passes chat_id 0.
    let unlink = parse(&set_chat_discussion_group(RequestId(10), 0, -200));
    assert_eq!(unlink["chat_id"], 0);
    assert_eq!(unlink["discussion_chat_id"], -200);
}

#[test]
fn available_reactions_all_some_and_none() {
    let all = parse(&set_chat_available_reactions(
        RequestId(11),
        5,
        &ChatAvailableReactions::All {
            max_reaction_count: 3,
        },
    ));
    assert_eq!(all["@type"], "setChatAvailableReactions");
    assert_eq!(
        all["available_reactions"]["@type"],
        "chatAvailableReactionsAll"
    );
    assert_eq!(all["available_reactions"]["max_reaction_count"], 3);
    let some = parse(&set_chat_available_reactions(
        RequestId(12),
        5,
        &ChatAvailableReactions::Some {
            reactions: vec![
                ReactionType::emoji("👍"),
                ReactionType::CustomEmoji {
                    custom_emoji_id: 77,
                },
                ReactionType::Paid,
            ],
            max_reaction_count: 11,
        },
    ));
    let list = some["available_reactions"]["reactions"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["@type"], "reactionTypeEmoji");
    assert_eq!(list[0]["emoji"], "👍");
    // int64 ids travel as strings.
    assert_eq!(list[1]["custom_emoji_id"], "77");
    assert_eq!(list[2]["@type"], "reactionTypePaid");
    let none = parse(&set_chat_available_reactions(
        RequestId(13),
        5,
        &ChatAvailableReactions::none(),
    ));
    assert_eq!(
        none["available_reactions"]["@type"],
        "chatAvailableReactionsSome"
    );
    assert!(
        none["available_reactions"]["reactions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

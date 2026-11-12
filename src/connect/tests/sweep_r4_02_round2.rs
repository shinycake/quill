//! Sweep fixes (r4-02, fix round 2): a stage holds the request it waits
//! for, a refusal ends in an error, and nothing is kept beside the stage.
use super::sweep_s3_payments_1::Flaky;
use crate::ids::ChatId;
use crate::state::fetch::Fetch;
use serde_json::json;

fn folder_list() -> serde_json::Value {
    json!({"@type":"recommendedChatFolders","chat_folders":[]})
}

#[test]
fn recommended_folders_are_asked_once_while_out_and_again_after_a_failure() {
    let mut t = Flaky::new();
    assert!(t.driver.fetch_recommended_chat_folders().unwrap().is_some());
    assert!(t.driver.session.chat_list.recommended_folders.is_loading());
    assert!(t.driver.fetch_recommended_chat_folders().unwrap().is_none());
    assert_eq!(t.sender.count("getRecommendedChatFolders"), 1);
    t.fail("getRecommendedChatFolders", "TIMEOUT");
    assert!(
        t.driver
            .session
            .chat_list
            .recommended_folders
            .error()
            .is_some()
    );
    assert!(t.driver.fetch_recommended_chat_folders().unwrap().is_some());
    t.answer("getRecommendedChatFolders", folder_list());
    assert!(matches!(
        t.driver.session.chat_list.recommended_folders,
        Fetch::Ready(_)
    ));
}

#[test]
fn a_refused_recommended_folders_send_ends_in_an_error() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getRecommendedChatFolders"]);
    assert!(t.driver.fetch_recommended_chat_folders().is_err());
    assert!(
        t.driver
            .session
            .chat_list
            .recommended_folders
            .error()
            .is_some()
    );
}

#[test]
fn the_share_box_chat_list_is_asked_once_while_out() {
    let mut t = Flaky::new();
    t.driver.fetch_folder_share(3).unwrap();
    t.driver.fetch_folder_share(3).unwrap();
    assert_eq!(t.sender.count("getChatsForChatFolderInviteLink"), 1);
    t.fail("getChatsForChatFolderInviteLink", "TIMEOUT");
    assert!(
        t.driver.session.chat_list.folder_shares[&3]
            .chats
            .error()
            .is_some()
    );
    t.driver.fetch_folder_share(3).unwrap();
    assert_eq!(t.sender.count("getChatsForChatFolderInviteLink"), 2);
}

#[test]
fn the_delete_box_chat_list_is_asked_once_while_out() {
    let mut t = Flaky::new();
    assert!(
        t.driver
            .fetch_chat_folder_chats_to_leave(3)
            .unwrap()
            .is_some()
    );
    assert!(
        t.driver
            .fetch_chat_folder_chats_to_leave(3)
            .unwrap()
            .is_none()
    );
    assert_eq!(t.sender.count("getChatFolderChatsToLeave"), 1);
    t.fail("getChatFolderChatsToLeave", "TIMEOUT");
    assert!(
        t.driver
            .fetch_chat_folder_chats_to_leave(3)
            .unwrap()
            .is_some()
    );
    assert_eq!(t.sender.count("getChatFolderChatsToLeave"), 2);
}

#[test]
fn a_contacts_delete_that_half_went_out_is_not_reported_as_failed() {
    let mut t = Flaky::new();
    t.driver.session.users_state.contacts = Fetch::Ready(vec![31, 32]);
    t.sender.refuse(&["removeContacts"]);
    assert_eq!(t.driver.delete_synced_contacts().unwrap(), 1);
    assert_eq!(t.sender.count("clearImportedContacts"), 1);
    assert!(t.driver.session.users_state.contacts_notice.is_some());
}

#[test]
fn a_refused_business_bot_search_shows_an_error_with_retry() {
    let mut t = Flaky::new();
    t.sender.refuse(&["searchChatsOnServer"]);
    assert!(t.driver.lookup_business_bot("@somebot").is_err());
    assert_eq!(t.sender.count("searchPublicChat"), 1, "the first went out");
    let lookup = &t.driver.session.settings.business_extra.lookup;
    assert!(lookup.error().is_some(), "the field shows the failure");
    assert!(lookup.done());
    t.sender.refuse(&[]);
    t.driver.lookup_business_bot("@somebot").unwrap();
    assert!(!t.driver.session.settings.business_extra.lookup.done());
}

#[test]
fn a_second_community_with_a_photo_does_not_replace_the_first() {
    let mut t = Flaky::new();
    for id in [5, 6] {
        t.feed(json!({"@type":"updateNewChat","chat":{"id":id,"title":"C","type":{"@type":"chatTypePrivate","user_id":id},"unread_count":0}}));
    }
    let first = t
        .driver
        .create_community_with_photo(ChatId(5), "A", false, Some("a.png".into()))
        .unwrap();
    assert!(first.is_some());
    let second = t
        .driver
        .create_community_with_photo(ChatId(6), "B", false, Some("b.png".into()))
        .unwrap();
    assert!(second.is_none(), "one picture waits at a time");
    assert_eq!(t.sender.count("createCommunity"), 1);
}

#[test]
fn a_create_without_a_photo_does_not_replace_a_waiting_photo_either() {
    let mut t = Flaky::new();
    for id in [5, 6] {
        t.feed(json!({"@type":"updateNewChat","chat":{"id":id,"title":"C","type":{"@type":"chatTypePrivate","user_id":id},"unread_count":0}}));
    }
    assert!(!t.driver.community_photo_create_out());
    t.driver
        .create_community_with_photo(ChatId(5), "A", false, Some("a.png".into()))
        .unwrap();
    assert!(t.driver.community_photo_create_out());
    let second = t
        .driver
        .create_community_with_photo(ChatId(6), "B", false, None)
        .unwrap();
    assert!(second.is_none());
    assert_eq!(t.sender.count("createCommunity"), 1);
    assert!(
        t.driver.community_photo_create_out(),
        "A's picture still waits"
    );
}

#[test]
fn topic_icons_ask_once_even_when_the_answer_is_empty_and_again_after_a_failure() {
    let mut t = Flaky::new();
    assert!(t.driver.load_forum_topic_icons().unwrap().is_some());
    assert!(t.driver.load_forum_topic_icons().unwrap().is_none());
    t.answer(
        "getForumTopicDefaultIcons",
        json!({"@type":"stickers","stickers":[]}),
    );
    assert!(
        t.driver.load_forum_topic_icons().unwrap().is_none(),
        "empty is an answer"
    );
    assert_eq!(t.sender.count("getForumTopicDefaultIcons"), 1);
    // A failed ask is asked again at the next open.
    let mut t = Flaky::new();
    t.driver.load_forum_topic_icons().unwrap();
    t.fail("getForumTopicDefaultIcons", "TIMEOUT");
    assert!(t.driver.session.threads.forum_topic_icons.error().is_some());
    assert!(t.driver.load_forum_topic_icons().unwrap().is_some());
}

#[test]
fn status_lists_are_asked_once_while_out_and_end_in_an_error() {
    let mut t = Flaky::new();
    t.driver.load_emoji_status_choices().unwrap();
    t.driver.load_emoji_status_choices().unwrap();
    assert_eq!(t.sender.count("getRecentEmojiStatuses"), 1);
    assert_eq!(t.sender.count("getDefaultEmojiStatuses"), 1);
    t.fail("getDefaultEmojiStatuses", "TIMEOUT");
    assert!(
        t.driver
            .session
            .stickers
            .emoji
            .default_status_ids
            .error()
            .is_some()
    );
    t.driver.load_emoji_status_choices().unwrap();
    assert_eq!(t.sender.count("getDefaultEmojiStatuses"), 2);
    assert_eq!(t.sender.count("getRecentEmojiStatuses"), 1, "still out");
}

#[test]
fn a_refused_status_list_send_ends_in_an_error() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getThemedEmojiStatuses"]);
    assert!(t.driver.load_emoji_status_choices().is_err());
    assert!(
        t.driver
            .session
            .stickers
            .emoji
            .themed_status_ids
            .error()
            .is_some()
    );
}

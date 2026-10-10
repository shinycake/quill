//! Service text wording: chats, members, pins, wallpapers, boosts, topics, games.
use super::*;

wording!(
    basic_group_created,
    BY_DANA,
    r#"{"@type":"messageBasicGroupChatCreate","title":"Weekend","member_user_ids":[2,3]}"#,
    "Dana Cole created the group \u{AB}Weekend\u{BB}"
);
wording!(
    supergroup_created,
    BY_DANA,
    r#"{"@type":"messageSupergroupChatCreate","title":"Weekend"}"#,
    "Dana Cole created the group \u{AB}Weekend\u{BB}"
);
wording!(
    channel_created,
    IN_CHANNEL,
    r#"{"@type":"messageSupergroupChatCreate","title":"Launch"}"#,
    "Channel created"
);
wording!(
    title_changed,
    BY_DANA,
    r#"{"@type":"messageChatChangeTitle","title":"Brunch Club"}"#,
    "Dana Cole changed group name to \u{AB}Brunch Club\u{BB}"
);
wording!(
    title_changed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatChangeTitle","title":"Launch News"}"#,
    "Channel name was changed to \u{AB}Launch News\u{BB}"
);
wording!(
    photo_changed,
    BY_DANA,
    r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Dana Cole updated group photo"
);
wording!(
    photo_changed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Channel photo updated"
);
wording!(
    photo_removed,
    BY_DANA,
    r#"{"@type":"messageChatDeletePhoto"}"#,
    "Dana Cole removed group photo"
);
wording!(
    photo_removed_channel,
    IN_CHANNEL,
    r#"{"@type":"messageChatDeletePhoto"}"#,
    "Channel photo removed"
);
wording!(
    owner_left,
    BY_DANA,
    r#"{"@type":"messageChatOwnerLeft","new_owner_user_id":2}"#,
    "Omar Haddad will become the new main admin in 7 days if Dana Cole does not return."
);
wording!(
    owner_changed,
    BY_DANA,
    r#"{"@type":"messageChatOwnerChanged","new_owner_user_id":2}"#,
    "Dana Cole made Omar Haddad the new main admin of the group."
);
wording!(
    protected_content_enabled,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":false,"new_has_protected_content":true}"#,
    "Dana Cole disabled sharing in this chat"
);
wording!(
    protected_content_disabled_by_me,
    BY_ME,
    r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":true,"new_has_protected_content":false}"#,
    "You enabled sharing in this chat"
);
wording!(
    protected_content_request,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":false}"#,
    "Dana Cole would like to enable sharing in this chat, which includes:\nForwarding messages\nSaving photos and videos\nCopying messages"
);
wording!(
    protected_content_request_expired,
    BY_DANA,
    r#"{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":true}"#,
    "Sharing enable request has expired"
);
wording!(
    members_added_one,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[2]}"#,
    "Dana Cole added Omar Haddad"
);
wording!(
    members_added_many,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[2,3,9]}"#,
    "Dana Cole added Omar Haddad, Lea Stern and Idan Birman"
);
wording!(
    member_added_self_is_a_join,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[1]}"#,
    "Dana Cole joined the group"
);
wording!(
    member_added_you,
    BY_DANA,
    r#"{"@type":"messageChatAddMembers","member_user_ids":[9]}"#,
    "Dana Cole added you to this group"
);
wording!(
    member_removed,
    BY_DANA,
    r#"{"@type":"messageChatDeleteMember","user_id":2}"#,
    "Dana Cole removed Omar Haddad"
);
wording!(
    member_left,
    BY_DANA,
    r#"{"@type":"messageChatDeleteMember","user_id":1}"#,
    "Dana Cole left the group"
);
wording!(
    joined_by_link,
    BY_DANA,
    r#"{"@type":"messageChatJoinByLink"}"#,
    "Dana Cole joined the group via invite link"
);
wording!(
    joined_by_request,
    BY_DANA,
    r#"{"@type":"messageChatJoinByRequest"}"#,
    "Dana Cole was accepted to the group"
);
wording!(
    your_join_request_approved,
    BY_ME,
    r#"{"@type":"messageChatJoinByRequest"}"#,
    "Your request to join the group was approved"
);
wording!(
    group_upgraded_to,
    BY_DANA,
    r#"{"@type":"messageChatUpgradeTo","supergroup_id":42}"#,
    "This group was upgraded to a supergroup"
);
wording!(
    group_upgraded_from,
    BY_DANA,
    r#"{"@type":"messageChatUpgradeFrom","title":"Weekend","basic_group_id":7}"#,
    "The group \u{AB}Weekend\u{BB} was upgraded to a supergroup"
);
wording!(
    pinned_text_is_cut_at_sixteen_characters,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":100}"#,
    "Dana Cole pinned \"Lunch at noon, b\u{2026}\""
);
wording!(
    pinned_short_text_is_kept,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":101}"#,
    "Dana Cole pinned \"Short note\""
);
wording!(
    pinned_media,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":102}"#,
    "Dana Cole pinned a photo"
);
wording!(
    pinned_unloaded,
    BY_DANA,
    r#"{"@type":"messagePinMessage","message_id":999}"#,
    "Dana Cole pinned a message"
);
wording!(
    wallpaper_set_by_other,
    BY_DANA,
    r#"{"@type":"messageChatSetBackground","old_background_message_id":0,"background":{"@type":"chatBackground","dark_theme_dimming":0},"only_for_self":false}"#,
    "Dana Cole set a new wallpaper for this chat"
);
wording!(
    wallpaper_set_by_me_for_self,
    BY_ME,
    r#"{"@type":"messageChatSetBackground","old_background_message_id":0,"background":{"@type":"chatBackground"},"only_for_self":true}"#,
    "You set a new wallpaper for this chat"
);
wording!(
    theme_changed,
    BY_DANA,
    r#"{"@type":"messageChatSetTheme","theme":{"@type":"chatThemeEmoji","name":"☃"}}"#,
    "Dana Cole changed the chat theme to \u{2603}"
);
wording!(
    theme_disabled,
    BY_DANA,
    r#"{"@type":"messageChatSetTheme","theme":null}"#,
    "Dana Cole disabled the chat theme"
);
wording!(
    gift_theme_set_by_me,
    BY_ME,
    r#"{"@type":"messageChatSetTheme","theme":{"@type":"chatThemeGift","gift_theme":{"@type":"giftChatTheme","gift":{"@type":"upgradedGift","title":"Plush Pepe","number":12}}}}"#,
    "You set Plush Pepe #12 as a new theme for this chat."
);
wording!(
    boost_applied,
    BY_DANA,
    r#"{"@type":"messageChatBoost","boost_count":3}"#,
    "Dana Cole boosted the group 3 times"
);
wording!(
    boost_applied_once,
    BY_DANA,
    r#"{"@type":"messageChatBoost","boost_count":1}"#,
    "Dana Cole boosted the group"
);
wording!(
    boost_applied_by_me,
    BY_ME,
    r#"{"@type":"messageChatBoost","boost_count":2}"#,
    "You boosted the group"
);
wording!(
    topic_created,
    BY_DANA,
    r#"{"@type":"messageForumTopicCreated","name":"Ideas","is_name_implicit":false,"icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"}}"#,
    "The topic \"Ideas\" was created"
);
wording!(
    topic_renamed,
    BY_DANA,
    r#"{"@type":"messageForumTopicEdited","name":"Plans","edit_icon_custom_emoji_id":false,"icon_custom_emoji_id":"0"}"#,
    "Dana Cole renamed the topic to \"Plans\""
);
wording!(
    topic_icon_changed,
    BY_DANA,
    r#"{"@type":"messageForumTopicEdited","name":"","edit_icon_custom_emoji_id":true,"icon_custom_emoji_id":"5312"}"#,
    "Dana Cole changed the topic icon"
);
wording!(
    topic_closed,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsClosedToggled","is_closed":true}"#,
    "Dana Cole closed the topic"
);
wording!(
    topic_reopened,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsClosedToggled","is_closed":false}"#,
    "Dana Cole reopened the topic"
);
wording!(
    topic_hidden,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsHiddenToggled","is_hidden":true}"#,
    "Topic hidden"
);
wording!(
    topic_unhidden,
    BY_DANA,
    r#"{"@type":"messageForumTopicIsHiddenToggled","is_hidden":false}"#,
    "Topic unhidden"
);
wording!(
    profile_photo_suggested,
    BY_DANA,
    r#"{"@type":"messageSuggestProfilePhoto","photo":{"@type":"chatPhoto","sizes":[]}}"#,
    "Dana Cole suggests this photo for your Telegram profile."
);
wording!(
    birthdate_suggested,
    BY_DANA,
    r#"{"@type":"messageSuggestBirthdate","birthdate":{"@type":"birthdate","day":6,"month":10,"year":1990}}"#,
    "Dana Cole suggests you add your date of birth: 6 October 1990"
);
wording!(
    custom_action,
    BY_DANA,
    r#"{"@type":"messageCustomServiceAction","text":"Welcome aboard"}"#,
    "Welcome aboard"
);
wording!(
    game_score_with_title,
    BY_DANA,
    r#"{"@type":"messageGameScore","game_message_id":300,"game_id":"5","score":120}"#,
    "Dana Cole scored 120 in Lumberjack"
);
wording!(
    game_score_without_title,
    BY_ME,
    r#"{"@type":"messageGameScore","game_message_id":999,"game_id":"5","score":15}"#,
    "You scored 15"
);
wording!(
    managed_bot_created,
    BY_DANA,
    r#"{"@type":"messageManagedBotCreated","bot_user_id":2,"manager_bot_user_id":3}"#,
    "Dana Cole created a bot Omar Haddad."
);

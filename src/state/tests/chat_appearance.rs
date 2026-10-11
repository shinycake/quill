//! State reducer tests: a channel's or group's appearance updates.
use super::common::*;

#[test]
fn chat_accent_update_stores_name_color_profile_color_and_icons() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAccentColors","chat_id":-1001,"accent_color_id":9,"background_custom_emoji_id":"555","upgraded_gift_colors":null,"profile_accent_color_id":4,"profile_background_custom_emoji_id":"666"}"#,
    );
    let accent = session.chats_state.chat_accents[&-1001];
    assert_eq!(accent.accent_color_id, 9);
    assert_eq!(accent.background_custom_emoji_id, 555);
    let look = session.chats_state.chat_looks[&-1001];
    assert_eq!(look.profile_accent_color_id, 4);
    assert_eq!(look.profile_background_custom_emoji_id, 666);
    assert_eq!(look.emoji_status_id, 0);
}

#[test]
fn chat_emoji_status_update_keeps_the_profile_color() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAccentColors","chat_id":-1001,"accent_color_id":1,"background_custom_emoji_id":"0","profile_accent_color_id":2,"profile_background_custom_emoji_id":"0"}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatEmojiStatus","chat_id":-1001,"emoji_status":{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"321"},"expiration_date":0}}"#,
    );
    let look = session.chats_state.chat_looks[&-1001];
    assert_eq!(look.emoji_status_id, 321);
    assert_eq!(look.profile_accent_color_id, 2);

    // A later accent update does not drop the status.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatAccentColors","chat_id":-1001,"accent_color_id":1,"background_custom_emoji_id":"0","profile_accent_color_id":-1,"profile_background_custom_emoji_id":"0"}"#,
    );
    let look = session.chats_state.chat_looks[&-1001];
    assert_eq!(look.emoji_status_id, 321);
    assert_eq!(look.profile_accent_color_id, -1);

    // Removing the status clears it.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatEmojiStatus","chat_id":-1001}"#,
    );
    assert_eq!(session.chats_state.chat_looks[&-1001].emoji_status_id, 0);
}

#[test]
fn palettes_keep_the_boost_levels_colors_need() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAccentColors","colors":[{"@type":"accentColor","id":12,"built_in_accent_color_id":3,"light_theme_colors":[1122867],"dark_theme_colors":[],"min_channel_chat_boost_level":5}],"available_accent_color_ids":[12]}"#,
    );
    assert_eq!(
        session.chats_state.name_accent_colors[0].min_channel_level,
        5
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateProfileAccentColors","colors":[{"@type":"profileAccentColor","id":7,"light_theme_colors":{"@type":"profileAccentColors","palette_colors":[255],"background_colors":[],"story_colors":[]},"dark_theme_colors":{"@type":"profileAccentColors","palette_colors":[],"background_colors":[],"story_colors":[]},"min_supergroup_chat_boost_level":3,"min_channel_chat_boost_level":6}],"available_accent_color_ids":[7]}"#,
    );
    let color = &session.chats_state.profile_accent_colors[0];
    assert_eq!(
        (color.min_supergroup_level, color.min_channel_level),
        (3, 6)
    );
}

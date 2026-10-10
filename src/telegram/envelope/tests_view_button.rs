use super::message_link_preview::{parse_link_preview, view_button_label};
use serde_json::{Value, json};

fn label(preview_type: Value) -> Option<&'static str> {
    view_button_label(&preview_type)
}

#[test]
fn chat_previews_say_channel_group_or_join_request() {
    let chat = |kind: &str, join: bool| json!({"@type": "linkPreviewTypeChat", "type": {"@type": kind}, "creates_join_request": join});
    assert_eq!(
        label(chat("inviteLinkChatTypeChannel", false)),
        Some("View channel")
    );
    assert_eq!(
        label(chat("inviteLinkChatTypeSupergroup", false)),
        Some("View group")
    );
    assert_eq!(
        label(chat("inviteLinkChatTypeBasicGroup", false)),
        Some("View group")
    );
    assert_eq!(
        label(chat("inviteLinkChatTypeChannel", true)),
        Some("Request to Join")
    );
}

#[test]
fn user_previews_tell_bots_from_people() {
    assert_eq!(
        label(json!({"@type": "linkPreviewTypeUser", "is_bot": true})),
        Some("View bot")
    );
    assert_eq!(
        label(json!({"@type": "linkPreviewTypeUser", "is_bot": false})),
        Some("Send message")
    );
}

#[test]
fn entity_previews_carry_their_desktop_labels() {
    for (kind, expected) in [
        ("linkPreviewTypeMessage", "View message"),
        ("linkPreviewTypeStory", "View story"),
        ("linkPreviewTypeTheme", "View theme"),
        ("linkPreviewTypeChannelBoost", "Boost"),
        ("linkPreviewTypeGroupCall", "Join call"),
        ("linkPreviewTypeWebApp", "Launch"),
        ("linkPreviewTypeStickerSet", "View stickers"),
    ] {
        assert_eq!(label(json!({"@type": kind})), Some(expected), "{kind}");
    }
    assert_eq!(
        label(json!({"@type": "linkPreviewTypeVideoChat", "is_live_stream": true})),
        Some("Live stream")
    );
    assert_eq!(
        label(json!({"@type": "linkPreviewTypeVideoChat", "is_live_stream": false})),
        Some("Video chat")
    );
}

#[test]
fn articles_and_media_cards_have_no_button() {
    for kind in [
        "linkPreviewTypeArticle",
        "linkPreviewTypePhoto",
        "linkPreviewTypeVideo",
        "linkPreviewTypeUnsupported",
    ] {
        assert_eq!(label(json!({"@type": kind})), None, "{kind}");
    }
}

#[test]
fn parsed_preview_keeps_the_button() {
    let value = json!({
        "@type": "linkPreview",
        "url": "https://t.me/telegram",
        "display_url": "t.me/telegram",
        "site_name": "Telegram",
        "title": "Telegram News",
        "type": {"@type": "linkPreviewTypeChat", "type": {"@type": "inviteLinkChatTypeChannel"}, "creates_join_request": false},
    });
    let (preview, _) = parse_link_preview(Some(&value));
    assert_eq!(preview.unwrap().view_button, Some("View channel"));
}

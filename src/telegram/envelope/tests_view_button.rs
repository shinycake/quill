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

fn preview_of(preview_type: Value) -> super::message_link_preview::LinkPreview {
    let value = json!({
        "@type": "linkPreview",
        "url": "https://t.me/bg/sky",
        "display_url": "t.me/bg/sky",
        "site_name": "Telegram",
        "title": "Wallpaper",
        "type": preview_type,
    });
    parse_link_preview(Some(&value)).0.unwrap()
}

fn doc(name: &str, mime: &str) -> Value {
    json!({
        "@type": "document", "file_name": name, "mime_type": mime,
        "thumbnail": {"@type": "thumbnail", "width": 90, "height": 160,
            "file": {"@type": "file", "id": 12, "size": 3}},
        "document": {"@type": "file", "id": 11, "size": 2048}
    })
}

#[test]
fn wallpaper_card_has_its_button_only_with_a_document() {
    let with_document = preview_of(json!({
        "@type": "linkPreviewTypeBackground",
        "document": doc("sky.jpg", "image/jpeg"),
        "background_type": {"@type": "backgroundTypeWallpaper", "is_blurred": false, "is_moving": false}
    }));
    assert_eq!(with_document.view_button, Some("View wallpaper"));
    assert_eq!(with_document.kind.picture_files().len(), 1);
    let colors_only = preview_of(json!({
        "@type": "linkPreviewTypeBackground",
        "document": null,
        "background_type": {"@type": "backgroundTypeFill",
            "fill": {"@type": "backgroundFillSolid", "color": 255}}
    }));
    assert_eq!(colors_only.view_button, None);
    assert!(matches!(
        colors_only.kind,
        super::message_link_preview::LinkPreviewKind::Background(_)
    ));
}

#[test]
fn theme_card_has_its_button_only_with_a_desktop_theme_file() {
    let desktop = preview_of(json!({
        "@type": "linkPreviewTypeTheme",
        "documents": [doc("night.tdesktop-theme", "application/x-tgtheme-tdesktop")]
    }));
    assert_eq!(desktop.view_button, Some("View theme"));
    let other = preview_of(json!({
        "@type": "linkPreviewTypeTheme",
        "documents": [doc("night.attheme", "application/x-tgtheme-android")]
    }));
    assert_eq!(other.view_button, None);
}

#[test]
fn auction_card_button_follows_the_dates() {
    let preview = preview_of(json!({
        "@type": "linkPreviewTypeGiftAuction",
        "gift": {"@type": "gift", "id": "5", "auction_info": {"@type": "giftAuction", "id": "pepe", "gifts_per_round": 10, "start_date": 1000}},
        "auction_end_date": 2000
    }));
    assert_eq!(preview.button_label(500), Some("View"));
    assert_eq!(preview.button_label(1500), Some("Join"));
    assert_eq!(preview.button_label(2500), Some("View Results"));
}

#[test]
fn style_link_card_says_view_style() {
    let preview = preview_of(json!({
        "@type": "linkPreviewTypeTextCompositionStyle",
        "custom_emoji_id": "5368324170671202286"
    }));
    assert_eq!(preview.view_button, Some("View Style"));
    assert_eq!(
        preview.kind,
        super::message_link_preview::LinkPreviewKind::TextCompositionStyle {
            custom_emoji_id: 5368324170671202286
        }
    );
}

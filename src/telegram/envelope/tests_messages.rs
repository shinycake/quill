use super::tests_media::local_file_json;
use super::*;
use crate::ids::{ChatId, FileId};

#[test]
fn b1_message_game_parsed() {
    // `messageGame` (schema 1.8.67, line 5234): title, text, description
    // and short name are kept — the game launches via
    // `callbackQueryPayloadGame` (schema:7743).
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":308,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageGame","game":{"@type":"game","id":"1","short_name":"chess","title":"Chess","text":{"@type":"formattedText","text":"Challenge me!","entities":[]},"description":"A classic.","photo":null,"animation":null},"game_message_id":308,"failed_to_load":false,"not_found":false}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let MessageContent::Game(game) = &message.content else {
                panic!("expected Game, got {:?}", message.content)
            };
            assert_eq!(game.short_name, "chess");
            assert_eq!(game.title, "Chess");
            assert_eq!(game.text.text, "Challenge me!");
            assert_eq!(game.description, "A classic.");
            assert!(game.photo.sizes.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn view_messages_schema_matches_1_8_67() {
    let schema = include_str!("../../../schema/td_api.tl");
    let view = schema
        .lines()
        .find(|l| l.starts_with("viewMessages "))
        .expect("viewMessages");
    assert!(view.contains("message_ids:vector<int53>"));
    assert!(view.contains("source:MessageSource"));
    assert!(view.contains("force_read:Bool"));
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("updateChatReadOutbox "))
    );
    assert!(schema.lines().any(|l| l.starts_with("openChat ")));
    assert!(schema.lines().any(|l| l.starts_with("closeChat ")));
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("messageSourceChatHistory"))
    );
}

#[test]
fn last_message_positions_are_typed() {
    let json = r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"preview","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":true}]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::UpdateChatLastMessage {
            chat_id,
            last_message,
            positions,
        } => {
            assert_eq!(chat_id.0, 3);
            assert_eq!(
                last_message.unwrap().content,
                MessageContent::Text("preview".into())
            );
            assert_eq!(positions.len(), 1);
            assert_eq!(positions[0].order, 5);
            assert!(positions[0].is_pinned);
            assert_eq!(positions[0].list, ChatList::Main);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn message_text_parses_link_entities_and_article_preview() {
    let text = "see https://example.com and the notes";
    let url_at = text.find("https").unwrap();
    let notes_at = text.find("notes").unwrap();
    let thumb = local_file_json(7, "", false, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{{"@type":"textEntity","offset":{url_at},"length":19,"type":{{"@type":"textEntityTypeUrl"}}}},{{"@type":"textEntity","offset":{notes_at},"length":5,"type":{{"@type":"textEntityTypeTextUrl","url":"https://example.com/notes"}}}},{{"@type":"textEntity","offset":0,"length":3,"type":{{"@type":"textEntityTypeBold"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Preview body","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle","photo":{{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":90,"height":90,"progressive_sizes":[]}}]}}}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}}}"#,
        text_json = serde_json::to_string(text).unwrap(),
    );
    let env = parse_envelope(&json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::Text(content) = message.content else {
        panic!("expected text");
    };
    assert_eq!(content.text, text);
    assert_eq!(content.entities.len(), 3);
    assert!(matches!(
        content.entities[0].kind,
        crate::text::TextEntityKind::Url
    ));
    assert_eq!(
        content.entities[1].open_href(&content.text),
        Some("https://example.com/notes")
    );
    // Phase 4.1: style entities parse alongside links.
    assert!(matches!(
        content.entities[2].kind,
        crate::text::TextEntityKind::Bold
    ));
    assert_eq!(content.entities[2].utf8_start, 0);
    assert_eq!(content.entities[2].utf8_end, 3);
    let preview = content.link_preview.expect("preview");
    assert_eq!(preview.site_name, "Example");
    assert_eq!(preview.title, "A short story");
    assert_eq!(preview.description, "Preview body");
    assert_eq!(preview.url, "https://example.com/story");
    assert!(!preview.show_large_media);
    assert!(!preview.show_above_text);
    let photo = preview.photo.expect("article photo");
    assert_eq!(photo.thumb_size().map(|size| size.file_id), Some(FileId(7)));
    assert_eq!(message.files.len(), 1);
    assert_eq!(message.files[0].id, FileId(7));
}

#[test]
fn message_text_parses_custom_emoji_entities() {
    // "hi 😀 bye": 😀 is at UTF-16 offset 3, length 2. The zero-id entity
    // sits on a valid span ("hi") and is skipped for its id, not its range.
    let json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":4,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi 😀 bye","entities":[{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"12345"}},{"@type":"textEntity","offset":0,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":0}}]}}}}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::UpdateNewMessage(message) = env.payload else {
        panic!("expected message");
    };
    let MessageContent::Text(content) = message.content else {
        panic!("expected text");
    };
    assert_eq!(content.entities.len(), 1);
    assert!(matches!(
        content.entities[0].kind,
        crate::text::TextEntityKind::CustomEmoji {
            custom_emoji_id: 12345
        }
    ));
    assert_eq!(content.entities[0].utf8_start, 3);
    assert_eq!(content.entities[0].utf8_end, 7);
}

/// MED4: `webPageInstantView` (schema:4377) reuses the M2 `pageBlock*`
/// parser — same blocks, new payload.
#[test]
fn web_page_instant_view_parses_blocks() {
    let json = r#"{"@type":"webPageInstantView","blocks":[{"@type":"pageBlockTitle","title":{"@type":"richTextPlain","text":"Headline"}},{"@type":"pageBlockParagraph","text":{"@type":"richTextPlain","text":"Body"}}],"view_count":3,"version":2,"is_rtl":false,"is_full":true,"feedback_link":null}"#;
    let payload = parse_payload("webPageInstantView", json).unwrap();
    let EnvelopePayload::WebPageInstantView { rich } = payload else {
        panic!("expected WebPageInstantView, got {payload:?}");
    };
    assert!(rich.is_full);
    assert_eq!(rich.blocks.len(), 2);
}

/// MED4: embedded-player and album `linkPreviewType*` (schema:4392,
/// :4434/:4443/:4452) classify the card; `instant_view_version`
/// (schema:4570) gates the IV reader.
#[test]
fn link_preview_parses_embedded_player_kind() {
    let json = r#"{"@type":"linkPreview","url":"https://video.example/watch","display_url":"video.example","site_name":"Vids","title":"Clip","description":{"@type":"formattedText","text":"","entities":[]},"author":"","type":{"@type":"linkPreviewTypeEmbeddedVideoPlayer","url":"https://video.example/embed/1","thumbnail":null,"duration":95,"width":640,"height":360},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":false,"show_above_text":false,"instant_view_version":2}"#;
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let (preview, _) = parse_link_preview(Some(&value));
    let preview = preview.expect("preview");
    assert_eq!(preview.instant_view_version, 2);
    assert_eq!(
        preview.kind,
        LinkPreviewKind::EmbeddedPlayer {
            url: "https://video.example/embed/1".to_string(),
            duration_secs: 95,
            audio: false,
        }
    );
    assert!(preview.has_card());
}

/// MED4: album `linkPreviewTypeAlbum` (schema:4392) yields up to 4
/// thumbnails for the strip (photo sizes + video thumbnails).
#[test]
fn link_preview_parses_album_kind() {
    let thumb = r#"{"@type":"file","id":21,"size":100,"expected_size":100,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":true,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","unique_id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}"#;
    let empty_file = r#"{"@type":"file","id":0,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":true,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"","unique_id":"","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}"#;
    let json = [
            r#"{"@type":"linkPreview","url":"https://example.com/album","display_url":"example.com","#,
            r#""site_name":"","title":"Album","description":{"@type":"formattedText","text":"","entities":[]},"#,
            r#""author":"","type":{"@type":"linkPreviewTypeAlbum","media":["#,
            r#"{"@type":"linkPreviewAlbumMediaPhoto","photo":{"@type":"photo","has_stickers":false,"minithumbnail":null,"#,
            r#""sizes":[{"@type":"photoSize","type":"m","photo":"#,
            thumb,
            r#","width":90,"height":90,"progressive_sizes":[]}]}},"#,
            r#"{"@type":"linkPreviewAlbumMediaVideo","video":{"@type":"video","duration":5,"width":320,"height":180,"#,
            r#""file_name":"","mime_type":"","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"#,
            r#""thumbnail":{"@type":"thumbnail","format":{"@type":"thumbnailFormatJpeg"},"width":64,"height":36,"file":"#,
            thumb,
            r#"},"thumbnail_ts":0,"start_ts":0,"video":"#,
            empty_file,
            r#"}}],"caption":""},"#,
            r#""has_large_media":false,"show_large_media":false,"show_media_above_description":false,"#,
            r#""skip_confirmation":false,"show_above_text":false,"instant_view_version":0}"#,
        ]
        .concat();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let (preview, files) = parse_link_preview(Some(&value));
    let preview = preview.expect("preview");
    let LinkPreviewKind::Album { thumbnails } = &preview.kind else {
        panic!("expected Album kind, got {:?}", preview.kind);
    };
    assert_eq!(thumbnails.len(), 2);
    assert_eq!(files.len(), 2);
}

#[test]
fn message_reply_to_message_is_typed() {
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]},"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let reply = message.reply_to.expect("reply_to");
            assert_eq!(reply.chat_id.0, 11);
            assert_eq!(reply.message_id.0, 101);
            assert_eq!(
                reply.quote_text.as_deref(),
                Some("Hello from injected JSON.")
            );
            assert!(reply.is_same_chat(ChatId(11)));
        }
        other => panic!("{other:?}"),
    }
    let story = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"story","entities":[]}},"reply_to":{"@type":"messageReplyToStory","story_poster_chat_id":11,"story_id":3}}}"#,
        )
        .unwrap();
    match story.payload {
        EnvelopePayload::UpdateNewMessage(message) => assert_eq!(message.reply_to, None),
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("messageReplyToMessage "))
    );
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("inputMessageReplyToMessage "))
    );
    assert!(schema.lines().any(|l| l.starts_with("textQuote ")));
    assert!(schema.lines().any(|l| l.starts_with("inputTextQuote ")));
}

#[test]
fn update_message_content_is_typed() {
    let env = parse_envelope(
            r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED","entities":[]}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateMessageContent {
            chat_id,
            message_id,
            content,
            files,
        } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(message_id.0, 102);
            assert_eq!(content, MessageContent::Text("CANARY_EDITED".into()));
            assert!(files.is_empty());
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("updateMessageContent "))
    );
    assert!(schema.lines().any(|l| l.starts_with("editMessageText ")));
    assert!(schema.lines().any(|l| l.starts_with("editMessageCaption ")));
    assert!(schema.lines().any(|l| l.starts_with("deleteMessages ")));
}

#[test]
fn message_forward_info_and_messages_are_typed() {
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"fwd body","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let info = message.forward_info.expect("forward_info");
            assert_eq!(
                info.origin,
                MessageOrigin::HiddenUser {
                    sender_name: "Ada Lovelace".into()
                }
            );
            assert_eq!(info.date, 1_700_000_000);
        }
        other => panic!("{other:?}"),
    }
    let messages = parse_envelope(
            r#"{"@type":"messages","@extra":"34","total_count":2,"messages":[{"id":80,"chat_id":12,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":11},"date":1}},null]}"#,
        )
        .unwrap();
    match messages.payload {
        EnvelopePayload::Messages(parsed) => {
            assert_eq!(parsed.len(), 1);
            assert_eq!(parsed[0].id.0, 80);
            assert_eq!(parsed[0].chat_id.0, 12);
            assert!(matches!(
                parsed[0].forward_info.as_ref().map(|i| &i.origin),
                Some(MessageOrigin::User { user_id }) if user_id.0 == 11
            ));
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(schema.lines().any(|l| l.starts_with("forwardMessages ")));
    assert!(schema.lines().any(|l| l.starts_with("messages ")));
    assert!(schema.lines().any(|l| l.starts_with("messageForwardInfo ")));
    assert!(schema.lines().any(|l| l.starts_with("messageOriginUser ")));
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("messageOriginHiddenUser "))
    );
}

#[test]
fn message_interaction_info_and_update_are_typed() {
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"react me","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":4,"forward_count":1,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":true}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            let info = message.interaction_info.expect("interaction_info");
            assert_eq!(info.view_count, 4);
            assert_eq!(info.forward_count, 1);
            let chips = info.emoji_chips();
            assert_eq!(chips.len(), 2);
            assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 3"));
            assert!(chips[0].is_chosen);
            assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
            assert!(!chips[1].is_chosen);
            assert!(info.chosen_emoji("❤"));
            assert!(!info.chosen_emoji("👍"));
        }
        other => panic!("{other:?}"),
    }
    let update = parse_envelope(
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        )
        .unwrap();
    match update.payload {
        EnvelopePayload::UpdateMessageInteractionInfo {
            chat_id,
            message_id,
            interaction_info,
        } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(message_id.0, 101);
            let info = interaction_info.expect("interaction_info");
            assert_eq!(info.emoji_chips()[0].chip_label().as_deref(), Some("👍 1"));
            assert!(info.chosen_emoji("👍"));
        }
        other => panic!("{other:?}"),
    }
    let cleared = parse_envelope(
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":null}"#,
        )
        .unwrap();
    match cleared.payload {
        EnvelopePayload::UpdateMessageInteractionInfo {
            interaction_info, ..
        } => assert_eq!(interaction_info, None),
        other => panic!("{other:?}"),
    }
    let after_unreact = toggle_chosen_emoji_reaction(
        Some(&MessageInteractionInfo {
            reactions: Some(MessageReactions {
                reactions: vec![MessageReaction {
                    reaction_type: ReactionType::emoji("❤"),
                    total_count: 3,
                    is_chosen: true,
                }],
                are_tags: false,
            }),
            ..MessageInteractionInfo::default()
        }),
        "❤",
    );
    assert!(!after_unreact.chosen_emoji("❤"));
    assert_eq!(
        after_unreact.emoji_chips()[0].chip_label().as_deref(),
        Some("❤ 2")
    );
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("updateMessageInteractionInfo "))
    );
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("messageInteractionInfo "))
    );
    assert!(schema.lines().any(|l| l.starts_with("messageReactions ")));
    assert!(schema.lines().any(|l| l.starts_with("messageReaction ")));
    assert!(schema.lines().any(|l| l.starts_with("reactionTypeEmoji ")));
    assert!(schema.lines().any(|l| l.starts_with("addMessageReaction ")));
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("removeMessageReaction "))
    );
}

#[test]
fn message_is_pinned_and_update_are_typed() {
    let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"is_pinned":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"pinned","entities":[]}}}}"#,
        )
        .unwrap();
    match env.payload {
        EnvelopePayload::UpdateNewMessage(message) => {
            assert!(message.is_pinned);
            assert_eq!(message.id.0, 101);
        }
        other => panic!("{other:?}"),
    }
    let update = parse_envelope(
        r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
    )
    .unwrap();
    match update.payload {
        EnvelopePayload::UpdateMessageIsPinned {
            chat_id,
            message_id,
            is_pinned,
        } => {
            assert_eq!(chat_id.0, 11);
            assert_eq!(message_id.0, 101);
            assert!(!is_pinned);
        }
        other => panic!("{other:?}"),
    }
    let schema = include_str!("../../../schema/td_api.tl");
    assert!(
        schema
            .lines()
            .any(|l| l.starts_with("updateMessageIsPinned "))
    );
    assert!(schema.lines().any(|l| l.starts_with("pinChatMessage ")));
    assert!(schema.lines().any(|l| l.starts_with("unpinChatMessage ")));
}

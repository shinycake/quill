//! Screenshot demos: privacy media.
#![deny(clippy::indexing_slicing)]

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demos::video_fixtures::{apply_ready_video_note_send, apply_ready_video_send};
use crate::ui::drafts::apply_ready_drafts;
use crate::ui::history::apply_ready_albums;
use crate::ui::message_text::{apply_ready_link_preview, apply_ready_text_entities};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use crate::ui::*;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::ids::{ChatId, MessageId};
use quill::settings::ThemeChoice;
use std::sync::atomic::Ordering;

register_demos![
    // Received photo album plus an own-sent album and a multi-attach composer.
    DemoSpec::chats(
        "ready-albums",
        "screenshot demo — received album and own-sent album"
    )
    .setup(QuillApp::demo_ready_albums)
    .attachments(|| {
        attachments(&[
            ("demo-thumb.png", AttachmentKind::Photo),
            ("demo-clip.mp4", AttachmentKind::Video),
        ])
    }),
    // Restored private-chat composer draft (`draftMessage`).
    DemoSpec::chats(
        "ready-drafts",
        "screenshot demo — restored private-chat draft"
    )
    .setup(QuillApp::demo_ready_drafts),
    // Link entities + web page (`linkPreview`) card (injected, no live Telegram).
    DemoSpec::chats(
        "ready-link-preview",
        "screenshot demo — link + web page preview"
    )
    .setup(QuillApp::demo_ready_link_preview),
    // Slice S3: Privacy overlay (Settings → Privacy) — five rules,
    // read-date setting, blocked list (injected, no live Telegram).
    DemoSpec::chats(
        "ready-privacy",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_privacy),
    // README showcase scene: a populated account (generated avatars and
    // photos, a lively group conversation); `QUILL_DEMO_SHOWCASE` picks
    // the view.
    DemoSpec::chats("ready-showcase", "screenshot demo — showcase")
        .setup(QuillApp::demo_ready_showcase),
    // Text-entity demo (injected, no live Telegram): a message with mixed
    // entities (bold/italic/underline/strikethrough/spoiler/code/pre, incl.
    // nested runs) plus a photo whose caption carries entities (Phase 4.1).
    DemoSpec::chats(
        "ready-text-entities",
        "screenshot demo — text entities in text + caption"
    )
    .setup(QuillApp::demo_ready_text_entities),
    DemoSpec::chats(
        "ready-unsupported-message",
        "screenshot demo — unsupported message (no live Telegram)"
    )
    .setup(QuillApp::demo_ready_unsupported_message),
    // Composer video-note attach chip plus an own-sent round note in history.
    DemoSpec::chats(
        "ready-video-note-send",
        "screenshot demo — local video note attach + own-sent round note"
    )
    .setup(QuillApp::demo_ready_video_note_send)
    .attachments(|| attachments(&[("demo-video-note.mp4", AttachmentKind::VideoNote)])),
    // Composer video attach chip plus an own-sent video playing in history.
    DemoSpec::chats(
        "ready-video-send",
        "screenshot demo — local video attach + own-sent playback"
    )
    .setup(QuillApp::demo_ready_video_send)
    .attachments(|| attachments(&[("demo-clip.mp4", AttachmentKind::Video)])),
];

impl QuillApp {
    fn demo_ready_albums(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_albums(session, &self.demo_ui.sink, &self.demo_ui.seq);
            // `QUILL_DEMO_ALBUM_REACTIONS=1`: reactions on the second item
            // of each album (any item of an album may carry them).
            if std::env::var_os("QUILL_DEMO_ALBUM_REACTIONS").is_some() {
                // And a received album without a caption: bubble-less.
                let bare = crate::ui::demo::demo_file_json(
                    77,
                    &crate::ui::demo::demo_thumb_png_path(),
                    true,
                );
                // Then an own album sent as a reply, and a forwarded one.
                let reply = r#","reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":801,"quote":null,"checklist_task_id":0,"poll_option_id":""}"#;
                let forward = r#","forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":11},"date":1790631000,"source":null,"public_service_announcement_type":""}"#;
                let photos = [
                    (821, false, 77003, ""),
                    (822, false, 77003, ""),
                    (831, true, 77004, reply),
                    (832, true, 77004, reply),
                    (841, false, 77005, forward),
                    (842, false, 77005, forward),
                ]
                .map(|(id, outgoing, album, extra)| {
                    format!(
                        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":{outgoing},"date":1790632500,"media_album_id":"{album}","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{bare},"width":320,"height":200,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}{extra}}}}}"#
                    )
                });
                let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                    self.demo_ui.sink.clone();
                // And a service notice with a reaction under it.
                let pinned = r#"{"@type":"updateNewMessage","message":{"id":851,"chat_id":11,"is_outgoing":false,"date":1790632560,"content":{"@type":"messagePinMessage","message_id":801}}}"#.to_string();
                for json in photos.iter().chain([&pinned]) {
                    if let Some(owned) =
                        quill::telegram::client::copy_and_parse(json, &self.demo_ui.seq, &sink)
                    {
                        session.apply(owned);
                    }
                }
                for (message_id, emoji, chosen) in [
                    (802, "😭", false),
                    (812, "❤", true),
                    (822, "😭", false),
                    (832, "👍", false),
                    (841, "🔥", false),
                    (851, "🎉", false),
                ] {
                    let json = format!(
                        r#"{{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":{message_id},"interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reactions":{{"@type":"messageReactions","reactions":[{{"@type":"messageReaction","type":{{"@type":"reactionTypeEmoji","emoji":"{emoji}"}},"total_count":1,"is_chosen":{chosen},"recent_sender_ids":[{{"@type":"messageSenderUser","user_id":11}}]}}],"are_tags":false,"can_get_added_reactions":true}}}}}}"#
                    );
                    if let Some(owned) =
                        quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &sink)
                    {
                        session.apply(owned);
                    }
                }
            }
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("album caption", window, cx);
        });
        self.set_note(StatusNote::silent(
            "screenshot demo — received album · own album",
        ));
    }

    /// A draft saved on another device for the open chat: the frame
    /// upkeep puts it in the composer (`adopt_remote_draft`).
    fn demo_ready_drafts(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_drafts(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent(
            "screenshot demo — draft from another device",
        ));
    }

    fn demo_ready_link_preview(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_link_preview(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::info("screenshot demo — link preview"));
    }

    fn demo_ready_showcase(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::showcase_demo as sc;
        let view = std::env::var("QUILL_DEMO_SHOWCASE").unwrap_or_default();
        let scene = match view.as_str() {
            "poll" => sc::Scene::Lunch,
            "poll-gallery" => sc::Scene::PollGallery,
            "player" | "accent" => sc::Scene::Maya,
            "channel" => sc::Scene::Channel,
            _ => sc::Scene::Hikers,
        };
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            sc::apply_ready_showcase(session, &self.demo_ui.sink, &self.demo_ui.seq, scene);
        }
        match view.as_str() {
            "reactions" => {
                if let Some(session) = self.demo_session.as_mut() {
                    super::reaction_fixtures::seed_reaction_options(
                        session,
                        ChatId(sc::HIKERS),
                        MessageId(sc::HIKERS_FIRST_MESSAGE),
                    );
                }
                self.message_ui.menu.open_at(MessageMenuState {
                    chat_id: ChatId(sc::HIKERS),
                    message_id: MessageId(sc::HIKERS_FIRST_MESSAGE),
                    position: point(px(560.), px(150.)),
                });
                self.message_ui.menu.expand_reactions();
            }
            "viewer" => {
                self.open_media_viewer(ChatId(sc::HIKERS), MessageId(sc::HIKERS_PHOTO_MESSAGE), cx);
            }
            "player" => {
                self.begin_track_playback(
                    PlaybackKind::Audio,
                    ChatId(sc::MAYA),
                    MessageId(sc::MAYA_AUDIO_MESSAGE),
                    214.0,
                    87.0,
                    std::path::Path::new("demo-track"),
                    cx,
                );
                self.pause_active_playback();
            }
            "appearance" => {
                self.appearance.theme = ThemeChoice::Dark;
                self.appearance.accent_rgb = 0x8b5cf6;
                self.appearance.wallpaper_rgb = Some(0x1b1230);
                self.open_settings_on(quill::settings_catalog::Page::Appearance, cx);
            }
            "accent" => {
                self.appearance.accent_rgb = 0x8b5cf6;
            }
            "calls" => self.demo_showcase_calls(),
            "contacts" => self.demo_showcase_contacts(),
            _ => {}
        }
        // The README captures carry no debug caption.
        self.connection.clear_note();
    }

    fn demo_ready_text_entities(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_text_entities(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent("screenshot demo — text entities"));
    }

    fn demo_ready_unsupported_message(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            for json in [
                r#"{"@type":"updateNewMessage","message":{"id":110,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageFutureFeature"}}}"#,
                r#"{"@type":"updateNewMessage","message":{"id":111,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageExpiredPhoto"}}}"#,
                r#"{"@type":"updateNewMessage","message":{"id":112,"chat_id":11,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageWidget","width":320,"height":180,"html":"<p>hi</p>","public_data":"","chat_data":"","widget_id":"7","caption":{"@type":"formattedText","text":"Today's scoreboard","entities":[]},"show_caption_above_media":false,"is_copy":false,"can_send_queries":true,"can_send_background_queries":false}}}"#,
            ] {
                if let Some(message) =
                    quill::telegram::client::copy_and_parse(json, &self.demo_ui.seq, &sink)
                {
                    session.apply(message);
                }
            }
            session.open_chat(ChatId(11));
        }
    }

    fn demo_ready_video_note_send(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_note_send(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent(
            "screenshot demo — video note attach · own round note",
        ));
    }

    fn demo_ready_video_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sending a clip", window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_send(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent(
            "screenshot demo — attach video · own clip playing",
        ));
    }

    fn demo_privacy(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Slice S3: Privacy overlay with injected rules, the read-date
        // setting, and the blocked list (no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_privacy(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_settings(Some(quill::settings_catalog::Page::Privacy), cx);
        self.set_note(StatusNote::silent(
            "screenshot demo — privacy settings (injected, no live Telegram)",
        ));
    }
}

//! Screenshot demos: privacy media.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::drafts::apply_ready_drafts;
use crate::ui::history::apply_ready_albums;
use crate::ui::inline_playback::{apply_ready_video_note_send, apply_ready_video_send};
use crate::ui::message_text::{apply_ready_link_preview, apply_ready_text_entities};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
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
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("album caption", window, cx);
        });
        self.connection.status_note = "screenshot demo — received album · own album".into();
    }

    fn demo_ready_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_drafts(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.restore_open_draft(window, cx);
        self.connection.status_note = "screenshot demo — draft restored".into();
    }

    fn demo_ready_link_preview(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_link_preview(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — link preview".into();
    }

    fn demo_ready_showcase(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::showcase_demo as sc;
        let view = std::env::var("QUILL_DEMO_SHOWCASE").unwrap_or_default();
        let scene = match view.as_str() {
            "poll" => sc::Scene::Lunch,
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
                    use quill::state::{MessageReactionOptions, ReactionChoice};
                    let emoji = |e: &str| ReactionChoice::Emoji(e.to_string());
                    session.message_reaction_options = Some(MessageReactionOptions {
                        chat_id: ChatId(sc::HIKERS),
                        message_id: MessageId(sc::HIKERS_FIRST_MESSAGE),
                        top: ["❤", "👍", "🔥", "😂", "😮", "😢", "🎉"]
                            .into_iter()
                            .map(emoji)
                            .collect(),
                        recent: vec![emoji("👏")],
                        popular: [
                            "🤔", "🙏", "👌", "😍", "🤯", "😱", "🥰", "🤩", "💯", "⚡", "🏆", "🤝",
                        ]
                        .into_iter()
                        .map(emoji)
                        .collect(),
                        allow_custom_emoji: false,
                    });
                }
                self.message_ui.menu = Some(MessageMenuState {
                    chat_id: ChatId(sc::HIKERS),
                    message_id: MessageId(sc::HIKERS_FIRST_MESSAGE),
                    position: point(px(560.), px(150.)),
                });
                self.message_ui.reactions_expanded = true;
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
                    cx,
                );
                self.pause_active_playback();
            }
            "appearance" => {
                self.appearance.theme = ThemeChoice::Dark;
                self.appearance.accent_rgb = 0x8b5cf6;
                self.appearance.wallpaper_rgb = Some(0x1b1230);
                self.settings.appearance_open = true;
            }
            "accent" => {
                self.appearance.accent_rgb = 0x8b5cf6;
            }
            _ => {}
        }
        // The README captures carry no debug caption.
        self.connection.status_note = String::new();
    }

    fn demo_ready_text_entities(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_text_entities(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — text entities".into();
    }

    fn demo_ready_unsupported_message(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            for json in [
                r#"{"@type":"updateNewMessage","message":{"id":110,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageFutureFeature"}}}"#,
                r#"{"@type":"updateNewMessage","message":{"id":111,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageExpiredPhoto"}}}"#,
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

    fn demo_ready_video_note_send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_note_send(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.playback.playing_video = Some(MessageId(721));
        self.playback.video_frames = vec![
            demo_media_allowlist().join("demo-gif-1.png"),
            demo_media_allowlist().join("demo-gif-2.png"),
        ];
        self.spawn_video_tick(cx);
        self.connection.status_note = "screenshot demo — video note attach · own round note".into();
    }

    fn demo_ready_video_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sending a clip", window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_send(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.playback.playing_video = Some(MessageId(701));
        self.playback.video_frames = vec![
            demo_media_allowlist().join("demo-gif-1.png"),
            demo_media_allowlist().join("demo-gif-2.png"),
        ];
        self.spawn_video_tick(cx);
        self.connection.status_note = "screenshot demo — attach video · own clip playing".into();
    }

    fn demo_privacy(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice S3: Privacy overlay with injected rules, the read-date
        // setting, and the blocked list (no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_privacy(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.privacy.open = true;
        self.connection.status_note =
            "screenshot demo — privacy settings (injected, no live Telegram)".into();
    }
}

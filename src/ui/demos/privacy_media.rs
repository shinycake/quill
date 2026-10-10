//! Screenshot demos: privacy media.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_media_allowlist, demo_sessions};
use crate::ui::drafts::apply_ready_drafts;
use crate::ui::history::apply_ready_albums;
use crate::ui::inline_playback::{apply_ready_video_note_send, apply_ready_video_send};
use crate::ui::message_text::{
    apply_ready_blockquote_expandable, apply_ready_caption_position, apply_ready_link_preview,
    apply_ready_preview_cards, apply_ready_text_entities,
};
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
    // Settings > Ask a Question (injected, no live Telegram).
    DemoSpec::chats(
        "ready-ask-question",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_settings_help(SettingsHelpDemo::AskQuestion, window, cx)),
    // Expandable block quotes (injected, no live Telegram): a short quote
    // fully visible and a long quote collapsed to 3 lines with a kit ghost
    // "Show more" affordance (parity:msg-blockquote-expandable).
    DemoSpec::chats(
        "ready-blockquote-expandable",
        "screenshot demo — expandable block quotes"
    )
    .setup(QuillApp::demo_ready_blockquote_expandable),
    // Bubble headers and footer (injected, no live Telegram): replies with
    // a colored sender name, a quote, a media thumbnail, a reply from another
    // chat and a deleted original; forwards from a user, a hidden account, a
    // channel and an imported message; "via @bot"; and the footer's
    // "edited" / pin / views / "imported" marks.
    DemoSpec::chats("ready-bubble-headers", "screenshot demo — bubble headers")
        .setup(QuillApp::demo_ready_bubble_headers),
    // MED4: photo messages with caption above vs below the media
    // (`show_caption_above_media`, injected, no live Telegram).
    DemoSpec::chats(
        "ready-caption-position",
        "screenshot demo — caption above vs below"
    )
    .setup(QuillApp::demo_ready_caption_position),
    // Composer core: the "Code Language" box over a fenced block.
    DemoSpec::chats("ready-code-language", "screenshot demo — file drop zones")
        .setup(QuillApp::demo_ready_code_language),
    // MED4: composer with a typed URL → detected-URL chip + preview
    // toggle (injected, no live Telegram).
    DemoSpec::chats(
        "ready-composer-preview",
        "screenshot demo — composer link preview chip"
    )
    .setup(QuillApp::demo_ready_composer_preview),
    // Restored private-chat composer draft (`draftMessage`).
    DemoSpec::chats(
        "ready-drafts",
        "screenshot demo — restored private-chat draft"
    )
    .setup(QuillApp::demo_ready_drafts),
    // B13: the warning before opening an executable file.
    DemoSpec::chats(
        "ready-file-open-confirm",
        "screenshot demo — security settings (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_file_open_confirm),
    // Topic and thread info cards, story statistics with public shares,
    // and public story search (injected, no live Telegram):
    // `QUILL_DEMO_FTS_VIEW=topic|thread|stats|search` (default `topic`).
    DemoSpec::chats(
        "ready-forum-thread-stories",
        "screenshot demo — topic info, story statistics and search"
    )
    .setup(QuillApp::demo_ready_forum_thread_stories),
    // Forums and Saved Messages sublists (injected, no live Telegram):
    // `QUILL_DEMO_FORUMS_SAVED_VIEW=sublists|sublist|tag|editor` shows the
    // sublist list, one sublist, a tag filter, or the forum topic editor
    // with its icon picker (default `sublists`).
    DemoSpec::chats(
        "ready-forums-saved",
        "screenshot demo — forums and saved sublists"
    )
    .setup(QuillApp::demo_ready_forums_saved),
    // Gift/giveaway cards in a chat (fixture).
    DemoSpec::chats("ready-gift-cards", "screenshot demo — gift cards (fixture)")
        .setup(|app, window, cx| app.demo_premium_gifts(PremiumGiftsDemo::GiftCards, window, cx)),
    // Received gifts grid with the details of one gift (fixture).
    DemoSpec::chats("ready-gifts", "screenshot demo — received gifts (fixture)")
        .setup(|app, window, cx| app.demo_premium_gifts(PremiumGiftsDemo::Gifts, window, cx)),
    // Link entities + web page (`linkPreview`) card (injected, no live Telegram).
    DemoSpec::chats(
        "ready-link-preview",
        "screenshot demo — link + web page preview"
    )
    .setup(QuillApp::demo_ready_link_preview),
    // The composer's `@` member suggestions.
    DemoSpec::chats("ready-mentions", "screenshot demo — @ member suggestions")
        .setup(QuillApp::demo_ready_mentions),
    // Read-only Premium features explainer (fixture).
    DemoSpec::chats(
        "ready-premium",
        "screenshot demo — Premium features (fixture)"
    )
    .setup(|app, window, cx| app.demo_premium_gifts(PremiumGiftsDemo::Premium, window, cx)),
    // MED4: embedded-player + album `linkPreview` cards in bubbles
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-preview-cards",
        "screenshot demo — embedded + album preview cards"
    )
    .setup(QuillApp::demo_ready_preview_cards),
    // Slice S3: Privacy overlay (Settings → Privacy) — five rules,
    // read-date setting, blocked list (injected, no live Telegram).
    DemoSpec::chats(
        "ready-privacy",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_privacy(PrivacyDemo::Privacy, window, cx)),
    // Who can call me: the editor with its Always/Never allow lists
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-privacy-calls",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_privacy(PrivacyDemo::PrivacyCalls, window, cx)),
    // B13: the Gifts privacy editor — who can show gifts, the gift icon
    // switch and the accepted gift types (injected, no live Telegram).
    DemoSpec::chats(
        "ready-privacy-gifts",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_privacy(PrivacyDemo::PrivacyGifts, window, cx)),
    // Message rendering leftovers (injected, no live Telegram): grouped
    // bubbles with joined corners, contact cards with Message / Add
    // contact / View contact, a dice, a location with its map tile,
    // locked paid media, a suggested profile photo with its buttons, the
    // media viewer header and the attach menu's Contact / Location panels.
    // `QUILL_DEMO_RENDERING_VIEW=bubbles|cards|service|viewer|contact|location`
    // (default `bubbles`).
    DemoSpec::chats(
        "ready-rendering-leftovers",
        "screenshot demo — rendering leftovers"
    )
    .setup(QuillApp::demo_ready_rendering_leftovers),
    // A bot chat whose reply keyboard comes from `updateChatReplyMarkup`
    // (message outside the window), with request buttons and the share
    // dialogs. `QUILL_DEMO_KEYBOARD_VIEW=keyboard|hidden|phone|users|chat|confirm`.
    DemoSpec::chats(
        "ready-reply-keyboard",
        "screenshot demo — bot reply keyboard"
    )
    .setup(QuillApp::demo_ready_reply_keyboard),
    // RTL composer demo: a Hebrew / mixed / multi-paragraph draft chosen by
    // `QUILL_DEMO_RTL=he|mixed|lines` (default `he`).
    DemoSpec::chats("ready-rtl-composer", "screenshot demo — RTL composer draft")
        .setup(QuillApp::demo_ready_rtl_composer),
    // RTL polish demo: Hebrew chat-list previews, search results, short and
    // multi-line Hebrew bubbles with their time footers, a Hebrew reply and a
    // pinned message. `QUILL_DEMO_RTL_VIEW=chat|search` (default `chat`).
    DemoSpec::chats("ready-rtl-polish", "screenshot demo — RTL polish")
        .setup(QuillApp::demo_ready_rtl_polish),
    // Private chat with the cards of the render-service-media slice:
    // suggested photo and birthday, expired media, live locations with
    // Stop sharing, and link previews with a View button.
    DemoSpec::chats(
        "ready-service-media",
        "screenshot demo — service and media cards"
    )
    .setup(QuillApp::demo_ready_service_media),
    // Service-message tour in a group: members, pins with excerpt,
    // photo change, calls, gifts, giveaways, topics, timers, boosts.
    DemoSpec::chats(
        "ready-service-messages",
        "screenshot demo — service messages"
    )
    .setup(QuillApp::demo_ready_service_messages),
    // B13: the session details view (application, system, IP address,
    // location) with Terminate (injected, no live Telegram).
    DemoSpec::chats(
        "ready-session-details",
        "screenshot demo — security settings (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_session_details),
    // Settings with the help rows and the version footer (injected, no
    // live Telegram).
    DemoSpec::chats(
        "ready-settings-help",
        "screenshot demo — privacy settings (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_settings_help(
        SettingsHelpDemo::SettingsHelp,
        window,
        cx
    )),
    // README showcase scene: a populated account (generated avatars and
    // photos, a lively group conversation); `QUILL_DEMO_SHOWCASE` picks
    // the view.
    DemoSpec::chats("ready-showcase", "screenshot demo — showcase")
        .setup(QuillApp::demo_ready_showcase),
    // Stars balance + transaction history dialog (fixture, no purchases).
    DemoSpec::chats("ready-stars", "screenshot demo — Stars (fixture)")
        .setup(|app, window, cx| app.demo_premium_gifts(PremiumGiftsDemo::Stars, window, cx)),
    // Text-entity demo (injected, no live Telegram): a message with mixed
    // entities (bold/italic/underline/strikethrough/spoiler/code/pre, incl.
    // nested runs) plus a photo whose caption carries entities (Phase 4.1).
    DemoSpec::chats(
        "ready-text-entities",
        "screenshot demo — text entities in text + caption"
    )
    .setup(QuillApp::demo_ready_text_entities),
    // Channel comments and reply threads (injected, no live Telegram):
    // `QUILL_DEMO_THREADS_VIEW=posts` shows channel posts with comment
    // bars, `thread` a post's comment thread in its discussion group,
    // `group` a group message with replies.
    DemoSpec::chats("ready-threads", "screenshot demo — comments and threads")
        .setup(QuillApp::demo_ready_threads),
    // Translation demo (injected, no live Telegram): the translate bar,
    // translated bubbles, the translate box, the language chooser and the
    // settings. `QUILL_DEMO_TRANSLATE_VIEW=bar|translated|box|rtl|selection|chooser|settings|skip`
    // (default `bar`).
    DemoSpec::chats("ready-translate", "screenshot demo — translation")
        .setup(QuillApp::demo_ready_translate),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrivacyDemo {
    Privacy,
    PrivacyGifts,
    PrivacyCalls,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsHelpDemo {
    SettingsHelp,
    AskQuestion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PremiumGiftsDemo {
    Stars,
    Gifts,
    Premium,
    GiftCards,
}

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

    fn demo_ready_blockquote_expandable(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_blockquote_expandable(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — expandable block quotes".into();
    }

    fn demo_ready_bubble_headers(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::bubble_header_demo::apply_ready_bubble_headers(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
            );
        }
        self.connection.status_note = "screenshot demo — bubble headers".into();
    }

    fn demo_ready_caption_position(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // MED4: caption above vs below the media.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_caption_position(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — caption position".into();
    }

    fn demo_ready_code_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // MED4b: composer link-preview chip — type a URL so the
        // detected-URL chip + preview controls render; inject a fake
        // prefetched preview (no live TDLib in demo mode) with large
        // media on offer so the size toggle renders too.
        let text = "Here is the fix:\n```\nfn main() {\n    println!(\"hi\");\n}\n```";
        // The fence becomes a code block in the field.
        self.set_composer_markup(text, window, cx);
        self.composer.update(cx, |input, cx| {
            input.set_selected_range(40..40, cx);
        });
        self.open_code_language_dialog(window, cx);
        if let Some(dialog) = self.composer_ui.code_language.as_ref() {
            dialog
                .input_for_demo()
                .update(cx, |input, cx| input.set_value("rust", window, cx));
        }
        self.connection.status_note = "screenshot demo — code language".into();
    }

    fn demo_ready_composer_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("see https://example.com/story", window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            session.composer_preview = Some(quill::state::ComposerLinkPreview {
                url: "https://example.com/story".to_string(),
                preview: Some(Some(quill::telegram::envelope::LinkPreview {
                    url: "https://example.com/story".to_string(),
                    display_url: "example.com".to_string(),
                    site_name: "Example".to_string(),
                    title: "A short story".to_string(),
                    description: "Telegram-style link preview for a private chat.".to_string(),
                    show_large_media: false,
                    has_large_media: true,
                    show_media_above_description: false,
                    show_above_text: false,
                    instant_view_version: 0,
                    photo: None,
                    kind: quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer {
                        url: "https://example.com/embed/1".to_string(),
                        duration_secs: 95,
                        audio: false,
                    },
                    view_button: None,
                })),
            });
        }
        self.connection.status_note = "screenshot demo — composer preview chip".into();
    }

    fn demo_ready_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_drafts(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.restore_open_draft(window, cx);
        self.connection.status_note = "screenshot demo — draft restored".into();
    }

    fn demo_ready_file_open_confirm(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.privacy.extra.file_open = Some(crate::ui::privacy_extra::FileOpenConfirm {
            path: std::path::PathBuf::from("invoice-2026.bin"),
            warning: quill::file_prefs::OpenWarning::Executable,
        });
        self.connection.status_note = "screenshot demo — file open warning".into();
    }

    fn demo_ready_forum_thread_stories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_FTS_VIEW").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::forum_thread_stories_demo::apply_ready_forum_thread_stories(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &view,
            );
        }
        match view.as_str() {
            "thread" => self.history.thread_info_open = true,
            "stats" => {
                self.open_story_viewer(ChatId(11), 5, cx);
                self.stories.stats_open = true;
            }
            "search" => {
                self.search_ui.input.update(cx, |input, cx| {
                    input.set_value("#sunset", window, cx);
                });
            }
            _ => self.history.topic_info_open = true,
        }
        self.connection.status_note =
            "screenshot demo — topic info, story statistics and search".into();
    }

    fn demo_ready_forums_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_FORUMS_SAVED_VIEW").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::forums_saved_demo::apply_ready_forums_saved(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &view,
            );
        }
        if view == "editor" {
            self.open_forum_topic_editor(ChatId(16), None, window, cx);
        }
        self.connection.status_note = "screenshot demo — forums and saved sublists".into();
    }

    fn demo_ready_link_preview(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_link_preview(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — link preview".into();
    }

    fn demo_ready_mentions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            for (id, first, last, username) in [
                (901, "Ada", "Lovelace", "ada"),
                (902, "Alan", "Turing", ""),
                (903, "Grace", "Hopper", "grace"),
            ] {
                let json = format!(
                    r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":{{"@type":"usernames","active_usernames":["{username}"],"disabled_usernames":[],"editable_username":"{username}","collectible_usernames":[]}},"accent_color_id":{},"type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusRecently"}}}}}}"#,
                    id % 7
                );
                if let Some(owned) =
                    quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink)
                {
                    session.apply(owned);
                }
            }
            // Demo chat A (11) is the chat the fixture opens.
            session.mention_search = Some(quill::state::MentionSearch {
                chat_id: session.open_chat.unwrap_or(ChatId(11)),
                query: "a".into(),
                user_ids: vec![901, 902, 903],
                request: None,
            });
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("Thanks @a", window, cx);
        });
        self.connection.status_note = "screenshot demo — @ member suggestions".into();
    }

    fn demo_ready_preview_cards(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // MED4: embedded-player + album preview cards.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_preview_cards(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — preview cards".into();
    }

    fn demo_ready_rendering_leftovers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_RENDERING_VIEW").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::rendering_demo::apply_ready_rendering_leftovers(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &view,
            );
        }
        match view.as_str() {
            "viewer" => {
                self.open_media_viewer(
                    ChatId(crate::ui::rendering_demo::CARDS),
                    MessageId(310),
                    cx,
                );
            }
            "contact" => self.open_share_contact_panel(window, cx),
            "location" => self.open_share_location_panel(window, cx),
            _ => {}
        }
        self.connection.status_note = "screenshot demo — rendering leftovers".into();
    }

    fn demo_ready_reply_keyboard(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_setup_reply_keyboard(cx);
    }

    fn demo_ready_rtl_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = match std::env::var("QUILL_DEMO_RTL").as_deref() {
            Ok("mixed") => "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop",
            Ok("lines") => "שלום עולם, מה קורה?\nHello world, how are you?\n123",
            Ok("empty") => "",
            _ => "שלום עולם, מה שלומך היום",
        };
        // `QUILL_DEMO_RTL_SELECT=<start>..<end>` (byte offsets) focuses the
        // composer with that range selected, `QUILL_DEMO_RTL_CARET=<at>`
        // with the caret there.
        let select = std::env::var("QUILL_DEMO_RTL_SELECT").ok().and_then(|v| {
            let (a, b) = v.split_once("..")?;
            Some(a.parse::<usize>().ok()?..b.parse::<usize>().ok()?)
        });
        // Right-to-left bubbles beside the composer: an incoming Hebrew
        // message (wraps) and an outgoing mixed one.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            let chat = session.open_chat.unwrap_or(ChatId(11));
            for (id, outgoing, body) in [
                (
                    901_i64,
                    false,
                    "שלום עולם, מה שלומך היום? זו הודעה ארוכה יותר כדי לראות את הטקסט נשבר לשורות בתוך הבועה.",
                ),
                (902, true, "היי, ההזמנה 12345 מוכנה ב-Telegram Desktop"),
                (904, false, "מחכה לעוד עדכונים ממנה"),
                (
                    903,
                    false,
                    "קישור https://example.com/he בתוך הודעה ארוכה עם מילה מודגשת וקוד לשורות נוספות בבועה",
                ),
            ] {
                // Entities by needle: a link, a bold word and inline code.
                let entity = |needle: &str, kind: &str| -> Option<String> {
                    let start = body.find(needle)?;
                    let from = quill::text::utf8_to_utf16_offset(body, start).ok()?;
                    let to = quill::text::utf8_to_utf16_offset(body, start + needle.len()).ok()?;
                    Some(format!(
                        r#"{{"@type":"textEntity","offset":{from},"length":{},"type":{{"@type":"{kind}"}}}}"#,
                        to - from
                    ))
                };
                let entities = if id == 903 {
                    [
                        entity("https://example.com/he", "textEntityTypeUrl"),
                        entity("מודגשת", "textEntityTypeBold"),
                        entity("וקוד", "textEntityTypeCode"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(",")
                } else {
                    String::new()
                };
                let body = serde_json::to_string(body).unwrap_or_default();
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":{outgoing},"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[{entities}]}}}}}}}}"#,
                    chat.0
                );
                if let Some(owned) =
                    quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink)
                {
                    session.apply(owned);
                }
            }
        }
        let caret = std::env::var("QUILL_DEMO_RTL_CARET")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());
        self.composer.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            if let Some(range) = select.clone().or(caret.map(|at| at..at)) {
                input.focus(window, cx);
                input.set_selected_range(range, cx);
            }
        });
        self.connection.status_note = "screenshot demo — RTL composer".into();
    }

    fn demo_ready_rtl_polish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_RTL_VIEW").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::rtl_demo::apply_ready_rtl_polish(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &view,
            );
        }
        if view == "search" {
            self.search_ui.input.update(cx, |input, cx| {
                input.set_value("שלום", window, cx);
                input.focus(window, cx);
            });
        }
        self.connection.status_note = "screenshot demo — RTL polish".into();
    }

    fn demo_ready_service_media(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::service_media_demo::apply_ready_service_media(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
            );
        }
        self.connection.status_note = "screenshot demo — service and media cards".into();
    }

    fn demo_ready_service_messages(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::service_demo::apply_ready_service_messages(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
            );
        }
        self.connection.status_note = "screenshot demo — service messages".into();
    }

    fn demo_ready_session_details(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // B13: session details view and the file-open warning.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            session.sessions = Some(demo_sessions());
            session.sessions_loading = false;
            session.sessions_error = None;
            session.privacy_data.inactive_session_ttl_days = Some(180);
        }
        self.privacy.sessions_open = true;
        self.privacy.extra.session_details = Some(123456789);
        self.connection.status_note = "screenshot demo — session details".into();
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

    fn demo_ready_threads(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let view = std::env::var("QUILL_DEMO_THREADS_VIEW").unwrap_or_default();
            crate::ui::threads_demo::apply_ready_threads(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &view,
            );
        }
        self.connection.status_note = "screenshot demo — comments and threads".into();
    }

    fn demo_ready_translate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_setup_translate(window, cx);
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

    fn demo_privacy(&mut self, demo: PrivacyDemo, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice S3: Privacy overlay with injected rules, the read-date
        // setting, and the blocked list (no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_privacy(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.privacy.open = true;
        if matches!(demo, PrivacyDemo::PrivacyGifts) {
            self.privacy.editor = Some(PrivacyEditorTarget::Rule(
                quill::telegram::requests_privacy::PrivacySettingKey::AutosaveGifts,
            ));
        }
        if matches!(demo, PrivacyDemo::PrivacyCalls) {
            self.privacy.editor = Some(PrivacyEditorTarget::Rule(
                quill::telegram::requests_privacy::PrivacySettingKey::AllowCalls,
            ));
        }
        self.connection.status_note =
            "screenshot demo — privacy settings (injected, no live Telegram)".into();
    }

    fn demo_settings_help(
        &mut self,
        demo: SettingsHelpDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.settings.open = true;
        if matches!(demo, SettingsHelpDemo::AskQuestion) {
            self.settings.page = Some(crate::ui::settings_account_ui::ASK_QUESTION_PAGE);
        }
        self.connection.status_note =
            "screenshot demo — settings help (injected, no live Telegram)".into();
    }

    fn demo_premium_gifts(
        &mut self,
        demo: PremiumGiftsDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let (sink, seq) = (&self.demo_ui.sink, &self.demo_ui.seq);
            match demo {
                PremiumGiftsDemo::Stars => {
                    crate::ui::premium_demo::apply_ready_stars(session, sink, seq)
                }
                PremiumGiftsDemo::Gifts => {
                    let select = std::env::var("QUILL_DEMO_GIFT_DETAILS").is_ok();
                    crate::ui::premium_demo::apply_ready_gifts(session, select, sink, seq)
                }
                PremiumGiftsDemo::Premium => crate::ui::premium_demo::apply_ready_premium(session),
                _ => crate::ui::premium_demo::apply_ready_gift_cards(session, sink, seq),
            }
            self.connection.status_note =
                "screenshot demo \u{2014} Stars, gifts and Premium (fixture)".into();
        }
    }
}

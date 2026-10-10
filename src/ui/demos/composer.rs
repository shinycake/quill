//! Screenshot demos: composer.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demo::{seed_ready_custom_emoji_session, seed_ready_send_media_session};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::ids::ChatId;

register_demos![
    DemoSpec::chat_list("ready-chats").setup(QuillApp::demo_ready_chats),
    DemoSpec::chat_list("ready-chats-composer").setup(QuillApp::demo_ready_chats_composer),
    DemoSpec::chat_list("ready-deep-link-info").setup(QuillApp::demo_ready_deep_link_info),
    DemoSpec::chat_list("ready-deep-link-invite").setup(QuillApp::demo_ready_deep_link_invite),
    // The share-link chat chooser (typed deep links).
    DemoSpec::chat_list("ready-deep-link-share").setup(QuillApp::demo_ready_deep_link_share),
    // Paste-image: composer with a pasted clipboard photo attachment chip
    // (injected, no live Telegram). Visible outcome of Ctrl/Cmd+V image paste.
    DemoSpec::ready(
        "ready-paste-image",
        seed_ready_send_media_session,
        "screenshot demo — paste clipboard image as photo attachment (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_paste_image)
    .attachments(|| attachments(&[("demo-thumb.png", AttachmentKind::Photo)])),
    // Composer attachment chip + outgoing photo/document (injected, no live Telegram).
    DemoSpec::ready(
        "ready-send-media",
        seed_ready_send_media_session,
        "screenshot demo — outgoing photo/document send (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_send_media)
    .attachments(|| attachments(&[("demo-notes.txt", AttachmentKind::Document)])),
    // Ready chat draft with typos underlined (red wavy).
    DemoSpec::chat_list("ready-spellcheck").setup(|app, window, cx| app.demo_spellcheck(
        SpellcheckDemo::Spellcheck,
        window,
        cx
    )),
    // Multi-line draft: typos underlined; link, mention, hashtag, command and code skipped.
    DemoSpec::chat_list("ready-spellcheck-panel").setup(|app, window, cx| app.demo_spellcheck(
        SpellcheckDemo::SpellcheckPanel,
        window,
        cx
    )),
    // Composer `:fire` with the emoji suggestion strip open.
    DemoSpec::chat_list("ready-suggest-emoji").setup(|app, window, cx| app.demo_suggest(
        SuggestDemo::SuggestEmoji,
        window,
        cx
    )),
    // Composer `#ru` with the recent-hashtag popup open.
    DemoSpec::chat_list("ready-suggest-hashtag").setup(|app, window, cx| app.demo_suggest(
        SuggestDemo::SuggestHashtag,
        window,
        cx
    )),
    // codex:composer-input: a draft whose formatting, mention tag and custom
    // emoji show in the field as they will be sent.
    // `QUILL_DEMO_WYSIWYG=rtl|wrap|select` picks a Persian draft, a long
    // bold line that wraps, or a selection across formats.
    DemoSpec::ready(
        "ready-composer-wysiwyg",
        seed_ready_custom_emoji_session,
        "screenshot demo — formatted composer field"
    )
    .setup(QuillApp::demo_ready_composer_wysiwyg),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpellcheckDemo {
    Spellcheck,
    SpellcheckPanel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SuggestDemo {
    SuggestHashtag,
    SuggestEmoji,
}

impl QuillApp {
    fn demo_ready_composer_wysiwyg(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // codex:composer-input: formats, a mention tag and a custom emoji
        // (id 4242 resolves in this seed) shown in the field.
        let variant = std::env::var("QUILL_DEMO_WYSIWYG").unwrap_or_default();
        let markup = match variant.as_str() {
            "rtl" => {
                "سلام **دوستان**، پیش‌نویس با _قالب‌بندی_ و ![😀](tg://emoji?id=4242) آماده است\nنسخهٔ ۲ برای [Ann](tg://user?id=777) ارسال شد"
            }
            "wrap" => {
                "**A long bold line that has to wrap inside the composer, measured with the bold font so no word sticks out past the edge** and plain text after it"
            }
            _ => {
                "Release notes: **bold**, _italic_, __underline__, ~~struck~~, ||spoiler||, `code` and a [link](https://telegram.org)\nThanks [Ann](tg://user?id=777) for the ![😀](tg://emoji?id=4242) review!\n> Quoted feedback stays a quote"
            }
        };
        self.set_composer_markup(markup, window, cx);
        let select = variant == "select";
        self.composer.update(cx, |input, cx| {
            input.focus(window, cx);
            if select {
                input.set_selected_range(15..43, cx);
            } else {
                let end = input.value().len();
                input.set_selected_range(end..end, cx);
            }
        });
        self.connection.status_note = "screenshot demo — formatted composer".into();
    }

    fn demo_ready_chats(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // `QUILL_DEMO_HISTORY_ANIM=…,panel|menu|select`: something over the
        // animated history, to check what the animation layer draws under it.
        use crate::ui::demo::demo_history_extra;
        let sticker = quill::ids::MessageId(crate::ui::demo::HISTORY_ANIM_STICKER);
        if demo_history_extra("panel") {
            self.pickers.media_panel.open = true;
            self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Emoji;
        }
        if demo_history_extra("menu") {
            self.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
                chat_id: ChatId(11),
                message_id: sticker,
                position: point(px(340.), px(380.)),
            });
        }
        if demo_history_extra("select") {
            self.toggle_forward_select(ChatId(11), sticker, false, cx);
        }
    }

    fn demo_ready_chats_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("hello from composer", window, cx);
        });
    }

    fn demo_ready_deep_link_info(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.links.deep_link_dialog = Some(
            "This link requires a newer version of Quill. Please update Quill to open it.".into(),
        );
    }

    fn demo_ready_deep_link_invite(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.links.deep_link_invite = Some(quill::state::DeepLinkState::InvitePreview {
            hash: "demo_invite".into(),
            title: "Rust Community".into(),
            member_count: 1248,
            creates_join_request: true,
            is_channel: false,
            generation: 1,
        });
    }

    fn demo_ready_deep_link_share(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.share.link_text = Some("https://example.com/article\nWorth a look".into());
    }

    fn demo_ready_paste_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("pasted from clipboard", window, cx);
        });
        self.connection.status_note =
            "screenshot demo — paste image → composer photo attachment".into();
    }

    fn demo_ready_send_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sending a photo too", window, cx);
        });
    }

    fn demo_spellcheck(
        &mut self,
        demo: SpellcheckDemo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_prefs.spellcheck_enabled = true;
        // Ignore persisted app words so this fixture always shows typos.
        self.spell.checker = Self::new_spellchecker(false).0;
        // Off macOS the fixture must not depend on the host's dictionaries.
        #[cfg(not(target_os = "macos"))]
        {
            self.spell.checker = std::sync::Arc::new(quill::spellcheck::SpellChecker::wordlist());
        }
        // `-panel`: a multi-line draft proving the skip rules — the
        // link, mention, hashtag, command and code stay unmarked.
        let draft = if matches!(demo, SpellcheckDemo::SpellcheckPanel) {
            "Teh quick brown fox has a speling error.\n\
             See https://exampel.com/tehh, ask @tehuser about #tehtag,\n\
             run /strat or `cargo biuld` \u{1F60A} and recieve it tomorow."
        } else {
            "Teh quick brown fox has a speling error"
        };
        self.composer.update(cx, |input, cx| {
            input.set_value(draft, window, cx);
        });
        self.spellcheck_now(cx);
    }

    fn demo_suggest(&mut self, demo: SuggestDemo, window: &mut Window, cx: &mut Context<Self>) {
        // In-memory fixtures; nothing is persisted.
        self.chat_prefs.suggest_emoji = true;
        self.composer_ui.suggest.hashtags = quill::suggest::RecentHashtags::default();
        for tag in [
            "#rustlang",
            "#rust",
            "#rustacean",
            "#ruby",
            "#gpui",
            "#rustlang",
        ] {
            self.composer_ui.suggest.hashtags.record_message(tag);
        }
        let draft = if matches!(demo, SuggestDemo::SuggestHashtag) {
            "shipping the new composer today #ru"
        } else {
            "that release was :fire"
        };
        self.composer.update(cx, |input, cx| {
            input.set_value(draft, window, cx);
            input.set_selected_range(draft.len()..draft.len(), cx);
        });
        self.sync_suggest_menu(cx);
    }
}

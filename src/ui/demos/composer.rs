//! Screenshot demos: composer.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_send_media_session;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::ids::ChatId;

register_demos![
    DemoSpec::chat_list("ready-chats").setup(QuillApp::demo_ready_chats),
    DemoSpec::chat_list("ready-chats-composer").setup(QuillApp::demo_ready_chats_composer),
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
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpellcheckDemo {
    Spellcheck,
    SpellcheckPanel,
}

impl QuillApp {
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
}

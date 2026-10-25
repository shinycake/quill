use super::*;

impl QuillApp {
    fn demo_reaction_packs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use quill::state::MessageReactionsState;
        use quill::telegram::envelope::{StickerFormat, StickerSetInfo};
        let (chat_id, message_id) = (ChatId(11), MessageId(101));
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui
            .seq
            .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
        crate::ui::reactions::apply_ready_reactions(session, &self.demo_ui.sink, &self.demo_ui.seq);
        crate::ui::demos::reaction_fixtures::seed_reaction_options(session, chat_id, message_id);
        if let MessageReactionsState::Ready(options) =
            &mut session.stickers.message_reaction_options
        {
            options.allow_custom_emoji = true;
        }
        session.payments.premium_option = Some(std::env::var_os("QUILL_DEMO_PREMIUM").is_some());
        let packs = [
            (1, "Fun faces", ["😀", "😎", "🤩", "🥳", "😇", "🤠"]),
            (2, "Animals", ["🐶", "🐱", "🦊", "🐼", "🐨", "🐸"]),
        ];
        for (id, title, emojis) in packs {
            let items: Vec<StickerItem> = emojis
                .iter()
                .zip(0..)
                .map(|(emoji, index)| StickerItem {
                    custom_emoji_id: Some(id * 10 + index),
                    id: id * 10 + index,
                    set_id: id,
                    emoji: (*emoji).into(),
                    width: 100,
                    height: 100,
                    format: StickerFormat::Webp,
                    file_id: FileId(0),
                    thumb_file_id: None,
                    thumb_width: 0,
                    thumb_height: 0,
                    requires_premium: false,
                    premium_animation: None,
                })
                .collect();
            session.stickers.emoji.installed_sets.push(StickerSetInfo {
                id,
                title: title.into(),
                name: title.into(),
                size: 6,
                is_installed: true,
                is_official: false,
                is_owned: false,
                covers: Vec::new(),
                cover_files: Vec::new(),
                is_viewed: true,
            });
            session.media.media_library.set_stickers.insert(id, items);
        }
        self.open_reaction_selector(chat_id, message_id, point(px(420.), px(160.)), window, cx);
    }
}

crate::ui::screenshot_demo::register_demos![
    // `QUILL_DEMO_PREMIUM=1` opens it on a Premium account instead.
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-reaction-packs",
        "screenshot demo — reaction selector · locked packs",
    )
    .setup(QuillApp::demo_reaction_packs),
];

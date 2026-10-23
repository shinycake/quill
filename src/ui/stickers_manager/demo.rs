//! Screenshot demo: the stickers manager box over the settings, with sets
//! in every tab.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::state::{Session, StickerTab};
use quill::telegram::envelope::{StickerFormat, StickerItem, StickerSetInfo};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    // Installed sets with their first sticker, count, reorder handle and
    // remove button; Trending has unread dots and Archived has sets too.
    DemoSpec::chats(
        "ready-stickers-manager",
        "screenshot demo — the stickers manager box"
    )
    .setup(QuillApp::demo_stickers_manager),
];

/// A set whose cover is the demo picture `cover_file`.
fn demo_set(
    session: &mut Session,
    template: &StickerItem,
    seq: &AtomicU64,
    sink: &Arc<dyn quill::diagnostics::DiagnosticSink>,
    id: i64,
    title: &str,
    size: i32,
    emoji: &str,
    installed: bool,
    viewed: bool,
) -> StickerSetInfo {
    let file_id = 9200 + id as i32;
    let json = format!(
        r#"{{"@type":"updateFile","file":{}}}"#,
        crate::ui::demo::demo_file_json(file_id, &crate::ui::demo::demo_thumb_png_path(), true)
    );
    if let Some(owned) = quill::telegram::client::copy_and_parse(&json, seq, sink) {
        session.apply(owned);
    }
    let mut cover = template.clone();
    cover.id = i64::from(file_id);
    cover.set_id = id;
    cover.file_id = quill::ids::FileId(file_id);
    cover.thumb_file_id = None;
    cover.format = StickerFormat::Webp;
    cover.emoji = emoji.into();
    StickerSetInfo {
        id,
        title: title.into(),
        name: title.replace(' ', ""),
        size,
        is_installed: installed,
        is_official: false,
        is_owned: false,
        covers: vec![cover],
        cover_files: Vec::new(),
        is_viewed: viewed,
    }
}

impl QuillApp {
    fn demo_stickers_manager(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        crate::ui::composer_ui::apply_ready_stickers(
            session,
            &self.demo_ui.sink,
            &self.demo_ui.seq,
        );
        let Some(template) = session.stickers.stickers.stickers.first().cloned() else {
            return;
        };
        let sink: Arc<dyn quill::diagnostics::DiagnosticSink> = self.demo_ui.sink.clone();
        let seq = &self.demo_ui.seq;
        let mut make =
            |id: i64, title: &str, size: i32, emoji: &str, installed: bool, viewed: bool| {
                demo_set(
                    session, &template, seq, &sink, id, title, size, emoji, installed, viewed,
                )
            };
        let installed = vec![
            make(101, "Cute Cats", 24, "🐱", true, true),
            make(102, "Space Pals", 18, "🚀", true, true),
            make(103, "Tiny Foods", 30, "🍕", true, true),
            make(104, "Reaction Memes", 12, "😂", true, true),
        ];
        let trending = vec![
            make(201, "Sleepy Sloths", 20, "🦥", false, false),
            make(102, "Space Pals", 18, "🚀", true, true),
            make(202, "Pixel Pets", 36, "👾", false, false),
        ];
        let archived = vec![
            make(301, "Old Classics", 16, "📼", false, true),
            make(302, "Winter 2022", 9, "⛄", false, true),
        ];
        let stickers = &mut session.stickers.stickers;
        stickers.sets = installed;
        stickers.installed_loaded = true;
        stickers.trending_total = trending.len();
        stickers.trending_next_offset = trending.len();
        stickers.trending = trending;
        stickers.archived = archived;
        stickers.open = true;
        stickers.tab = StickerTab::Installed;
        session.stickers.manager.open = true;
        self.settings.sticker_settings_open = true;
        self.connection
            .set_note(StatusNote::silent("screenshot demo — stickers manager"));
    }
}

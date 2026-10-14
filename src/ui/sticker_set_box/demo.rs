//! Screenshot demo: the set box on a set the user created, with the "+"
//! cell and the creator's footer.
use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::state::{StickerSetView, StickerSetViewStage};

register_demos![
    // An owned set open in the box: six stickers, the "+" cell, "Edit
    // Stickers" in the footer, ⋯ menu with Edit name / Reorder / Delete Pack.
    DemoSpec::chats(
        "ready-sticker-set-owned",
        "screenshot demo — an owned sticker set open in the set box"
    )
    .setup(QuillApp::demo_owned_sticker_set),
];

impl QuillApp {
    fn demo_owned_sticker_set(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui
                .seq
                .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
            crate::ui::composer_ui::apply_ready_stickers(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
            );
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            let thumb = crate::ui::demo::demo_thumb_png_path();
            let Some(template) = session.stickers.stickers.stickers.first().cloned() else {
                return;
            };
            let mut stickers = Vec::new();
            for (index, emoji) in ["😀", "👋", "🔥", "❤️", "🎉", "🤔"].iter().enumerate() {
                let id = 9100 + index as i32;
                let json = format!(
                    r#"{{"@type":"updateFile","file":{}}}"#,
                    crate::ui::demo::demo_file_json(id, &thumb, true)
                );
                if let Some(owned) =
                    quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &sink)
                {
                    session.apply(owned);
                }
                let mut item = template.clone();
                item.id = i64::from(id);
                item.file_id = quill::ids::FileId(id);
                item.thumb_file_id = None;
                item.format = quill::telegram::envelope::StickerFormat::Webp;
                item.emoji = (*emoji).into();
                item.set_id = 78;
                stickers.push(item);
            }
            for set in &mut session.stickers.stickers.sets {
                set.is_owned = true;
            }
            session.stickers.stickers.sets.push(quill::telegram::envelope::StickerSetInfo {
                id: 78,
                title: "My doodles".into(),
                name: "my_doodles_by_quill".into(),
                size: stickers.len() as i32,
                is_installed: true,
                is_official: false,
                is_owned: true,
            });
            let mut view = StickerSetView::loading(78);
            view.stage = StickerSetViewStage::Ready {
                title: "My doodles".into(),
                name: "my_doodles_by_quill".into(),
                installed: true,
                is_emoji: false,
                is_owned: true,
                stickers,
            };
            view.files_requested = true;
            session.stickers.sticker_set_view = Some(view);
        }
        self.message_ui.menu_ui.sticker_set_open = true;
        self.connection.status_note = "screenshot demo — your own sticker set".into();
    }
}

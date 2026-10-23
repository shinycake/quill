//! Screenshot demo: the set box on a set the user created, with the "+"
//! cell and the creator's footer.
use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
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
            for (index, emoji) in ["😀", "👋", "🔥", "❤️", "🎉", "🤔"].iter().enumerate()
            {
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
            session
                .media
                .media_library
                .set_stickers
                .insert(78, stickers.clone());
            session
                .stickers
                .stickers
                .sets
                .push(quill::telegram::envelope::StickerSetInfo {
                    id: 78,
                    title: "My doodles".into(),
                    name: "my_doodles_by_quill".into(),
                    size: stickers.len() as i32,
                    is_installed: true,
                    is_official: false,
                    is_owned: true,
                    covers: Vec::new(),
                    cover_files: Vec::new(),
                    is_viewed: true,
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
        // `QUILL_DEMO_STICKER_BOX=<state>` opens one of the editing states
        // for its screenshot: reorder, rename, delete, create, editor, video, panel.
        match std::env::var("QUILL_DEMO_STICKER_BOX").as_deref() {
            Ok("reorder") => self.sticker_box.reordering = true,
            Ok("rename") => {
                self.sticker_box.rename_input.update(_cx, |input, cx| {
                    input.set_value("My doodles", _window, cx);
                });
                self.sticker_box.rename_open = true;
            }
            Ok("delete") => {
                self.sticker_box.delete_sticker = self
                    .demo_session
                    .as_ref()
                    .and_then(|s| s.stickers.sticker_set_view.as_ref())
                    .and_then(|view| match &view.stage {
                        StickerSetViewStage::Ready { stickers, .. } => stickers.get(2).cloned(),
                        _ => None,
                    });
            }
            Ok("create") => {
                self.sticker_box.create = Some(super::state::CreateSticker {
                    set_id: 78,
                    kind: SetKind::Stickers,
                    path: std::path::PathBuf::from(crate::ui::demo::demo_thumb_png_path()),
                    format: quill::telegram::envelope::StickerFormat::Webp,
                    preview: Some(std::path::PathBuf::from(
                        crate::ui::demo::demo_thumb_png_path(),
                    )),
                    emojis: vec!["😀".into(), "🎨".into()],
                    video: None,
                });
            }
            // The sticker editor on a wide image: the transparent canvas,
            // the rounded frame and the corners menu.
            Ok("editor") => {
                let wide = image::open(crate::ui::demo::demo_thumb_png_path())
                    .ok()
                    .map(|image| {
                        let image = image.to_rgba8();
                        let (w, h) = image.dimensions();
                        image::imageops::crop_imm(&image, 0, h / 4, w, (h / 2).max(1)).to_image()
                    });
                self.edit_pixels_for_set(78, SetKind::Stickers, wide, _cx);
            }
            // The video editor on a generated clip for the set (needs
            // ffmpeg; without it the editor shows its wait note).
            Ok("video") => {
                let clip = std::env::temp_dir().join("quill-demo-sticker-clip.mp4");
                let args: Vec<String> = [
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=duration=5:size=640x360:rate=30",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ]
                .map(String::from)
                .into_iter()
                .chain([clip.to_string_lossy().into_owned()])
                .collect();
                if let Ok(job) = quill::video_encoder::h264_job("The demo clip", &args, 640 * 360) {
                    let _ = quill::video_encoder::quiet_command_for(&job.program)
                        .args(&job.args)
                        .output();
                }
                self.open_set_video_editor(
                    78,
                    SetKind::Stickers,
                    clip,
                    Some("😀".into()),
                    _window,
                    _cx,
                );
            }
            Ok("panel") => {
                self.message_ui.menu_ui.sticker_set_open = false;
                self.pickers.media_panel.open = true;
                self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Stickers;
            }
            _ => {}
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — your own sticker set"));
    }
}

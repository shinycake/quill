//! Send card demos. `QUILL_DEMO_SEND_CARD` picks the tray: `album`
//! (default: two photos and a video), `one`, `many`, `apart` (the album
//! sent as separate messages), `spoiler` (a hidden photo and the caption
//! above), `files` or `mixed`.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::{Context, Window};
use quill::composer::{AttachmentKind, ComposerAttachment};

register_demos![
    DemoSpec::chat_list("ready-send-card")
        .setup(QuillApp::demo_send_card)
        .attachments(send_card_files),
];

fn variant() -> String {
    std::env::var("QUILL_DEMO_SEND_CARD").unwrap_or_default()
}

fn send_card_files() -> Vec<ComposerAttachment> {
    use AttachmentKind::{Document, Photo, Video};
    let files: &[(&str, AttachmentKind)] = match variant().as_str() {
        "one" => &[("showcase/p1.jpg", Photo)],
        "many" => &[
            ("showcase/p1.jpg", Photo),
            ("showcase/p2.jpg", Photo),
            ("demo-clip-12s.mp4", Video),
            ("showcase/p3.jpg", Photo),
            ("showcase/p4.jpg", Photo),
            ("demo-thumb.png", Photo),
            ("showcase/p5.jpg", Photo),
        ],
        "files" => &[
            ("demo-notes.txt", Document),
            ("showcase/p6.jpg", Document),
            ("demo-voice.ogg", Document),
        ],
        "mixed" => &[
            ("showcase/p1.jpg", Photo),
            ("showcase/p2.jpg", Photo),
            ("demo-notes.txt", Document),
        ],
        _ => &[
            ("showcase/p1.jpg", Photo),
            ("showcase/p2.jpg", Photo),
            ("demo-clip-12s.mp4", Video),
        ],
    };
    let root = crate::ui::demo::demo_media_allowlist();
    files
        .iter()
        .filter_map(|(file, kind)| ComposerAttachment::pick(&root.join(file), *kind))
        .collect()
}

impl QuillApp {
    fn demo_send_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // screen-scope: the demo session never switches accounts.
        // window-scope: a demo has one window; the tray is its open chat's.
        cx.defer_in(window, |app, _, cx| {
            match variant().as_str() {
                "apart" => app.set_composer_group_media(Some(false)),
                "spoiler" => {
                    app.update_pending_attachment(1, |attachment| attachment.spoiler = true);
                    app.update_pending_send(|send| send.caption_above = true);
                }
                _ => {}
            }
            app.check_tray_videos(cx);
        });
    }
}

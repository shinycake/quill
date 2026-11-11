//! Send box demos. `QUILL_DEMO_SEND_CARD` picks the tray: `album`
//! (default: two photos and a video), `one`, `many`, `ten` (a full album,
//! whose narrow tiles fold their controls into a menu), `apart` (the album
//! sent as separate messages), `spoiler` (a hidden photo and the caption
//! above), `hd` (a photo big enough for "HD", chosen), `timer` (one video
//! with a self-destruct timer), `channel` (the album in a channel the
//! account owns: a price, and a cover on the video), `files` or `mixed`.

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
        "ten" => &[
            ("showcase/p1.jpg", Photo),
            ("showcase/p2.jpg", Photo),
            ("showcase/p3.jpg", Photo),
            ("showcase/p4.jpg", Photo),
            ("showcase/p5.jpg", Photo),
            ("showcase/p6.jpg", Photo),
            ("demo-thumb.png", Photo),
            ("demo-clip-12s.mp4", Video),
            ("showcase/p2.jpg", Photo),
            ("showcase/p1.jpg", Photo),
        ],
        "hd" => return hd_files(),
        "timer" => &[("demo-clip-12s.mp4", Video)],
        // Its tray is filled once the channel is open.
        "channel" => &[],
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

/// The album with its first photo scaled past the size "HD" is for.
fn hd_files() -> Vec<ComposerAttachment> {
    use AttachmentKind::{Photo, Video};
    let root = crate::ui::demo::demo_media_allowlist();
    let big = std::env::temp_dir().join(format!("quill-demo-hd-{}.jpg", std::process::id()));
    let scaled = image::open(root.join("showcase/p1.jpg")).ok().map(|photo| {
        photo
            .resize(1650, 1650, image::imageops::FilterType::Triangle)
            .into_rgb8()
            .save(&big)
    });
    let first = match scaled {
        Some(Ok(())) => big,
        _ => root.join("showcase/p1.jpg"),
    };
    [
        (first, Photo),
        (root.join("showcase/p2.jpg"), Photo),
        (root.join("demo-clip-12s.mp4"), Video),
    ]
    .into_iter()
    .filter_map(|(file, kind)| ComposerAttachment::pick(&file, kind))
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
                "hd" => {
                    app.update_pending_send(|send| send.hd_photos = true);
                }
                "timer" => {
                    app.update_pending_send(|send| {
                        send.self_destruct =
                            Some(quill::telegram::requests::SelfDestructSend::Timer(30))
                    });
                }
                "channel" => app.demo_channel_tray(cx),
                _ => {}
            }
            app.check_tray_videos(cx);
        });
    }

    /// The demo channel as one the account owns that takes paid media,
    /// with the album waiting in it: a price and a cover are set.
    fn demo_channel_tray(&mut self, cx: &mut Context<Self>) {
        use AttachmentKind::{Photo, Video};
        const CHANNEL: quill::ids::ChatId = quill::ids::ChatId(13);
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.open_chat(CHANNEL);
        if let Some(supergroup) = session.chat_supergroup(CHANNEL) {
            session.groups.supergroup_full_infos.insert(
                supergroup,
                quill::state::SupergroupFullInfoData {
                    admin: quill::telegram::envelope::SupergroupFullAdmin {
                        has_paid_media_allowed: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
        }
        if let Some(chat) = session.chats.get_mut(&CHANNEL.0) {
            chat.assume_member_status(Some(
                quill::telegram::envelope::ChannelMemberStatus::Creator,
            ));
        }
        self.sync_chat_ui(cx);
        let root = crate::ui::demo::demo_media_allowlist();
        let files: Vec<ComposerAttachment> = [
            ("showcase/p1.jpg", Photo),
            ("showcase/p2.jpg", Photo),
            ("demo-clip-12s.mp4", Video),
        ]
        .iter()
        .filter_map(|(file, kind)| ComposerAttachment::pick(&root.join(file), *kind))
        .collect();
        self.update_pending_attachments(|tray| *tray = files);
        self.update_pending_attachment(2, |attachment| {
            attachment.extras.cover = Some(root.join("showcase/cover1.png"))
        });
        self.update_pending_send(|send| send.paid_stars = 50);
        cx.notify();
    }
}

//! Connect-driver tests: the send box's media options (a GIF with a
//! caption, a video's cover and start frame, High Quality photos, paid
//! media), checked on the recorded request JSON.
use super::super::*;
use super::*;
use crate::composer::{AttachmentKind, ComposerAttachment, ComposerSnapshot, MediaSendOptions};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::SupergroupFullInfoData;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{ChannelMemberStatus, SupergroupFullAdmin};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dir: PathBuf,
}

impl Rig {
    /// A private chat 7 and a channel 13 the account owns, with paid media
    /// allowed when `paid_allowed`.
    fn new(paid_allowed: bool) -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
        for json in [
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":true},"unread_count":0}}"#,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":113,"status":{"@type":"chatMemberStatusCreator","is_member":true}}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &sink).unwrap())
                .unwrap();
        }
        driver.session.chats.get_mut(&13).unwrap().my_member_status =
            Some(ChannelMemberStatus::Creator);
        driver.session.groups.supergroup_full_infos.insert(
            113,
            SupergroupFullInfoData {
                admin: SupergroupFullAdmin {
                    has_paid_media_allowed: paid_allowed,
                    ..SupergroupFullAdmin::default()
                },
                ..SupergroupFullInfoData::default()
            },
        );
        Self {
            driver,
            recorder,
            dir,
        }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn last(&self) -> Value {
        serde_json::from_str(self.recorder.snapshot().last().expect("a request")).unwrap()
    }

    fn snapshot(&self, chat: i64, caption: &str, att: ComposerAttachment) -> ComposerSnapshot {
        ComposerSnapshot::capture_with_attachment(
            ChatId(chat),
            self.driver.session.view_generation,
            caption,
            Some(att),
        )
    }
}

fn demo_clip() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-clip.mp4")
}

#[test]
fn a_gif_file_goes_out_as_an_animation_with_its_caption() {
    let mut rig = Rig::new(false);
    let gif = rig.file("fun.gif", b"GIF89a");
    let mut att = ComposerAttachment::pick(&gif, AttachmentKind::Video).unwrap();
    att.extras.gif = true;
    let snap = rig
        .snapshot(7, "CANARYGIFCAP", att)
        .with_caption_above_media(true);
    rig.driver.send_snapshot(&snap).unwrap();
    let content = &rig.last()["input_message_content"];
    assert_eq!(content["@type"], "inputMessageAnimation");
    assert_eq!(content["caption"]["text"], "CANARYGIFCAP");
    assert_eq!(content["show_caption_above_media"], true);
    assert_eq!(
        content["animation"]["animation"]["path"],
        std::fs::canonicalize(&gif)
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn a_video_carries_its_cover_and_start_frame_where_covers_are_allowed() {
    let mut rig = Rig::new(false);
    let cover = rig.file("cover.png", &[1, 2, 3]);
    let mut att = ComposerAttachment::pick(&demo_clip(), AttachmentKind::Video).unwrap();
    att.extras.cover = Some(cover.clone());
    att.extras.start_timestamp = 2;
    // A channel takes covers.
    let snap = rig.snapshot(13, "", att.clone());
    rig.driver.send_snapshot(&snap).unwrap();
    let video = &rig.last()["input_message_content"]["video"];
    assert_eq!(video["cover"]["@type"], "inputFileLocal");
    assert_eq!(
        video["cover"]["path"],
        std::fs::canonicalize(&cover)
            .unwrap()
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(video["start_timestamp"], 2);
    // A private chat with a person does not: the cover is dropped, the
    // start frame still applies.
    let snap = rig.snapshot(7, "", att);
    rig.driver.send_snapshot(&snap).unwrap();
    let video = &rig.last()["input_message_content"]["video"];
    assert_eq!(video["cover"], Value::Null);
    assert_eq!(video["start_timestamp"], 2);
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn an_unreadable_photo_still_sends_with_either_quality() {
    let mut rig = Rig::new(false);
    let photo = rig.file("p.png", &[1, 2, 3]);
    let att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let path = att.send_path_str().unwrap();
    for hd in [false, true] {
        let snap = rig
            .snapshot(7, "", att.clone())
            .with_media_options(MediaSendOptions {
                hd_photos: hd,
                paid_stars: 0,
            });
        rig.driver.send_snapshot(&snap).unwrap();
        let content = &rig.last()["input_message_content"];
        assert_eq!(content["@type"], "inputMessagePhoto");
        assert_eq!(content["photo"]["photo"]["path"], path.as_str());
    }
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn a_big_photo_is_scaled_to_the_standard_limit_and_hd_keeps_more() {
    if crate::media_tools::command("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let mut rig = Rig::new(false);
    let big = rig.dir.join("big.png");
    let made = crate::media_tools::command("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=3000x2000",
            "-frames:v",
            "1",
        ])
        .arg(&big)
        .output()
        .is_ok_and(|out| out.status.success());
    if !made {
        return;
    }
    let att = ComposerAttachment::pick(&big, AttachmentKind::Photo).unwrap();
    let original = att.send_path_str().unwrap();
    let mut longer_sides = Vec::new();
    for hd in [false, true] {
        let snap = rig
            .snapshot(7, "", att.clone())
            .with_media_options(MediaSendOptions {
                hd_photos: hd,
                paid_stars: 0,
            });
        rig.driver.send_snapshot(&snap).unwrap();
        let sent = rig.last()["input_message_content"]["photo"]["photo"]["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(sent, original, "a copy at the limit is sent");
        let (width, height) = crate::composer::photo_size_of(std::path::Path::new(&sent)).unwrap();
        longer_sides.push(width.max(height));
    }
    assert_eq!(longer_sides, [1280, 2560]);
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn paid_media_is_one_message_with_every_item_and_the_price() {
    let mut rig = Rig::new(true);
    let photo = rig.file("a.png", &[1]);
    let photo_att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let mut video_att = ComposerAttachment::pick(&demo_clip(), AttachmentKind::Video).unwrap();
    video_att.extras.start_timestamp = 1;
    let snap = ComposerSnapshot::capture_album(
        ChatId(13),
        rig.driver.session.view_generation,
        "CANARYPAIDCAP",
        vec![photo_att.clone(), video_att.clone()],
    )
    .with_media_options(MediaSendOptions {
        hd_photos: false,
        paid_stars: 250,
    });
    rig.driver.send_snapshot(&snap).unwrap();
    let sent = rig.last();
    assert_eq!(sent["@type"], "sendMessage");
    assert_eq!(sent["chat_id"], 13);
    let content = &sent["input_message_content"];
    assert_eq!(content["@type"], "inputMessagePaidMedia");
    assert_eq!(content["star_count"], 250);
    assert_eq!(content["caption"]["text"], "CANARYPAIDCAP");
    let items = content["paid_media"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["type"]["@type"], "inputPaidMediaTypePhoto");
    assert_eq!(items[1]["type"]["@type"], "inputPaidMediaTypeVideo");
    assert_eq!(items[1]["type"]["start_timestamp"], 1);
    assert!(items[1]["type"]["duration"].as_i64().unwrap() > 0);
    assert!(items[1]["width"].as_i64().unwrap() > 0);

    // One photo is paid media too.
    let single = rig
        .snapshot(13, "", photo_att)
        .with_media_options(MediaSendOptions {
            hd_photos: true,
            paid_stars: 1,
        });
    rig.driver.send_snapshot(&single).unwrap();
    assert_eq!(
        rig.last()["input_message_content"]["paid_media"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn paid_media_is_refused_where_it_is_not_allowed() {
    let photo_name = "a.png";
    let paid = |stars| MediaSendOptions {
        hd_photos: false,
        paid_stars: stars,
    };
    // A channel that doesn't allow paid media.
    let mut rig = Rig::new(false);
    let photo = rig.file(photo_name, &[1]);
    let att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let snap = rig
        .snapshot(13, "", att.clone())
        .with_media_options(paid(5));
    assert_eq!(
        rig.driver.send_snapshot(&snap),
        Err(ConnectSendError::InvalidRequest)
    );
    // A private chat.
    let mut rig = Rig::new(true);
    let photo = rig.file(photo_name, &[1]);
    let att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let snap = rig.snapshot(7, "", att.clone()).with_media_options(paid(5));
    assert_eq!(
        rig.driver.send_snapshot(&snap),
        Err(ConnectSendError::InvalidRequest)
    );
    // A price out of range, a file, a GIF.
    rig.driver.session.messages.paid_media_star_max = 400;
    for bad in [0, 401] {
        let snap = rig
            .snapshot(13, "", att.clone())
            .with_media_options(paid(bad));
        // A zero price is a normal photo send.
        let result = rig.driver.send_snapshot(&snap);
        if bad == 0 {
            assert!(result.is_ok());
        } else {
            assert_eq!(result, Err(ConnectSendError::InvalidRequest));
        }
    }
    let doc = rig.file("n.txt", b"x");
    let file = ComposerAttachment::pick(&doc, AttachmentKind::Document).unwrap();
    let snap = rig.snapshot(13, "", file).with_media_options(paid(5));
    assert_eq!(
        rig.driver.send_snapshot(&snap),
        Err(ConnectSendError::InvalidRequest)
    );
    let gif_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-gif.gif");
    let mut gif = ComposerAttachment::pick(&gif_file, AttachmentKind::Video).unwrap();
    gif.extras.gif = true;
    let snap = rig.snapshot(13, "", gif).with_media_options(paid(5));
    assert_eq!(
        rig.driver.send_snapshot(&snap),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn a_soundless_video_is_an_animation_alone_and_a_video_in_an_album() {
    let mut rig = Rig::new(false);
    let mut silent = ComposerAttachment::pick(&demo_clip(), AttachmentKind::Video).unwrap();
    silent.extras.gif = true;
    // Alone: an animation.
    let snap = rig.snapshot(7, "alone", silent.clone());
    rig.driver.send_snapshot(&snap).unwrap();
    assert_eq!(
        rig.last()["input_message_content"]["@type"],
        "inputMessageAnimation"
    );
    // In an album: a plain video.
    let photo = rig.file("p.png", &[1]);
    let photo = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let album = ComposerSnapshot::capture_album(
        ChatId(7),
        rig.driver.session.view_generation,
        "",
        vec![photo, silent],
    );
    rig.driver.send_snapshot(&album).unwrap();
    let contents = &rig.last()["input_message_contents"];
    assert_eq!(contents[1]["@type"], "inputMessageVideo");
    let _ = std::fs::remove_dir_all(&rig.dir);
}

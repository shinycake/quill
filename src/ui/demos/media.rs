//! Screenshot demos: media.

use crate::ui::app::QuillApp;
use crate::ui::audio_playback::{apply_ready_audio, apply_ready_voice};
use crate::ui::composer_ui::apply_ready_stickers;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::demo::{
    seed_ready_custom_emoji_session, seed_ready_downloads_session, seed_ready_media_session,
};
use crate::ui::inline_playback::{apply_ready_gifs, apply_ready_video, apply_ready_video_note};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::voice::VoiceCapture;
use std::sync::atomic::Ordering;

register_demos![
    // Music file bubble with title, performer, cover, and Play/Pause.
    DemoSpec::chats("ready-audio", "screenshot demo — audio file playback")
        .setup(QuillApp::demo_ready_audio),
    // MED3 downloads-manager demo (injected, no live Telegram): the
    // `ReadyMedia` seed plus an actively downloading document (file 24,
    // 42% through `notes.txt`), a failed document (file 26, "Retry"
    // chip), and a completed one (file 25, `report.pdf`) sitting in the
    // recent list — with the downloads panel open beside the
    // conversation, showing per-file progress, cancel, open and
    // reveal-in-folder rows.
    DemoSpec::ready(
        "ready-downloads",
        seed_ready_downloads_session,
        "screenshot demo — downloads manager (injected updates, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_downloads),
    DemoSpec::ready(
        "ready-emoji-packs",
        seed_ready_custom_emoji_session,
        "screenshot demo — custom emoji rendered inline in message text"
    )
    .setup(QuillApp::demo_ready_emoji_packs),
    DemoSpec::chats(
        "ready-gif-playback",
        "screenshot demo — saved GIFs + history playback"
    )
    .setup(|app, window, cx| app.demo_gifs(GifsDemo::GifPlayback, window, cx)),
    // Saved-GIF panel + a playing animation in history (injected, no live Telegram).
    DemoSpec::chats(
        "ready-gifs",
        "screenshot demo — saved GIFs + history playback"
    )
    .setup(|app, window, cx| app.demo_gifs(GifsDemo::Gifs, window, cx)),
    DemoSpec::ready(
        "ready-media",
        seed_ready_media_session,
        "screenshot demo — photo/document (injected updates, no live Telegram)"
    ),
    DemoSpec::chats(
        "ready-sticker-playback",
        "screenshot demo — sticker panel + sticker in history"
    )
    .setup(|app, window, cx| app.demo_stickers(StickersDemo::StickerPlayback, window, cx))
    .timing(400, 7500),
    // Sticker panel + sticker in history (injected, no live Telegram).
    DemoSpec::chats(
        "ready-stickers",
        "screenshot demo — sticker panel + sticker in history"
    )
    .setup(|app, window, cx| app.demo_stickers(StickersDemo::Stickers, window, cx)),
    // Video bubble with Play/Pause in history (injected, no live Telegram).
    DemoSpec::chats("ready-video", "screenshot demo — video bubble playback")
        .setup(QuillApp::demo_ready_video),
    // Round video note with Play/Pause in history (injected, no live Telegram).
    DemoSpec::chats(
        "ready-video-note",
        "screenshot demo — round video note playback"
    )
    .setup(QuillApp::demo_ready_video_note),
    // Voice record bar + history playback (injected, no live Telegram).
    DemoSpec::chats(
        "ready-voice",
        "screenshot demo — voice record bar + history playback"
    )
    .setup(QuillApp::demo_ready_voice),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum StickersDemo {
    Stickers,
    StickerPlayback,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GifsDemo {
    Gifs,
    GifPlayback,
}

impl QuillApp {
    fn demo_ready_audio(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_audio(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.playback.playing_audio = Some(MessageId(801));
        self.connection.status_note = "screenshot demo — audio · playing".into();
    }

    fn demo_ready_downloads(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Exercise failed playback recovery without a network request.
        self.playback.pending_gif_play = Some((MessageId(205), FileId(26), String::new()));
        self.playback.pending_video_play =
            Some((MessageId(205), FileId(26), String::new(), 0, None));
        self.playback.pending_audio_play = Some((ChatId(11), MessageId(205), FileId(26), 1.));
        self.viewer.pending_play = Some((MessageId(205), FileId(26)));
        self.playback.pending_voice_play =
            Some((ChatId(11), MessageId(205), FileId(26), false, 1.));
        self.discard_stopped_media_playback(cx);
        assert!(
            self.playback.pending_gif_play.is_none()
                && self.playback.pending_video_play.is_none()
                && self.playback.pending_audio_play.is_none()
                && self.viewer.pending_play.is_none()
                && self.playback.pending_voice_play.is_none()
        );
        self.playback.pending_audio_play = Some((ChatId(11), MessageId(204), FileId(24), 1.));
        self.discard_stopped_media_playback(cx);
        assert!(self.playback.pending_audio_play.is_some());
        self.playback.pending_audio_play = None;
    }

    fn demo_ready_emoji_packs(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_gifs(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.gifs.open = false;
            session.emoji.open = true;
            session.emoji.installed_sets = [
                "Downloaded pack",
                "Downloading pack",
                "Updated pack",
                "Installing pack",
            ]
            .iter()
            .enumerate()
            .map(|(index, title)| quill::telegram::envelope::StickerSetInfo {
                id: index as i64 + 1,
                title: (*title).into(),
                name: (*title).into(),
                size: 1,
                is_installed: true,
                is_official: false,
            })
            .collect();
            session
                .emoji
                .pack_files
                .insert(1, vec![quill::ids::FileId(63)]);
            session
                .emoji
                .pack_files
                .insert(2, vec![quill::ids::FileId(62)]);
            session.downloading.insert(62);
            session.emoji.outdated_packs.insert(3);
            session.emoji.mutating_set = Some((4, true));
            session.settings.media_prefs.recent_emoji_packs = vec![2, 1];
        }
    }

    fn demo_ready_video(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.playback.playing_video = Some(MessageId(601));
        self.playback.video_frames = vec![
            demo_media_allowlist().join("demo-gif-1.png"),
            demo_media_allowlist().join("demo-gif-2.png"),
        ];
        self.spawn_video_tick(cx);
        self.connection.status_note = "screenshot demo — video · playing".into();
    }

    fn demo_ready_video_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_note(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.playback.playing_video = Some(MessageId(611));
        self.playback.video_frames = vec![
            demo_media_allowlist().join("demo-gif-1.png"),
            demo_media_allowlist().join("demo-gif-2.png"),
        ];
        self.spawn_video_tick(cx);
        self.connection.status_note = "screenshot demo — video note · playing".into();
    }

    fn demo_ready_voice(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_voice(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // One Play request must survive the download and start the note.
        let local = self.demo_session.as_ref().unwrap().files[&82].clone();
        let mut waiting = local.clone();
        waiting.local.path.clear();
        waiting.local.is_downloading_completed = false;
        self.demo_session
            .as_mut()
            .unwrap()
            .files
            .insert(82, waiting);
        self.toggle_voice_playback(ChatId(11), MessageId(91), FileId(82), true, 3., cx);
        assert!(self.playback.pending_voice_play.is_some());
        self.demo_session.as_mut().unwrap().files.insert(82, local);
        self.resume_pending_voice(cx);
        assert!(
            self.playback.pending_voice_play.is_none()
                && self.playback.playing_voice == Some(MessageId(91))
        );
        self.stop_voice_playback();
        let bars = vec![4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14];
        self.recording.voice_capture = Some(VoiceCapture::preview(
            demo_media_allowlist().join("demo-voice.ogg"),
            2,
            bars,
        ));
        self.playback.playing_voice = Some(MessageId(91));
        // MED2: demo shows the locked record bar + a transcribed note.
        self.recording.locked = true;
        self.connection.status_note =
            "screenshot demo — recording voice · locked · playing voice note".into();
    }

    fn demo_stickers(&mut self, demo: StickersDemo, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_stickers(session, &self.demo_ui.sink, &self.demo_ui.seq);
            if demo == StickersDemo::StickerPlayback {
                crate::ui::composer_ui::apply_ready_sticker_playback(
                    session,
                    &self.demo_ui.sink,
                    &self.demo_ui.seq,
                );
            }
        }
        // The panel's library holds the demo set's contents.
        if let Some(session) = self.demo_session.as_mut() {
            let mut stickers = session.stickers.stickers.clone();
            // Performance fixture: `QUILL_DEMO_STICKERS=<n>` fills the
            // picker with `n` distinct animated stickers (more than the
            // playback cache holds).
            let extra: i32 = std::env::var("QUILL_DEMO_STICKERS")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            if demo == StickersDemo::StickerPlayback && extra > 0 {
                let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
                let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                    self.demo_ui.sink.clone();
                let root = crate::ui::demo::demo_media_allowlist();
                for i in 0..extra {
                    let id = 9_000 + i;
                    let name = if i % 2 == 0 {
                        "demo-sticker.tgs"
                    } else {
                        "demo-sticker.webm"
                    };
                    let json = crate::ui::demo::demo_file_json(
                        id,
                        &root.join(name).to_string_lossy(),
                        true,
                    );
                    if let Some(owned) = quill::telegram::client::copy_and_parse(&json, &seq, &sink)
                    {
                        session.apply(owned);
                    }
                    let mut item = stickers[0].clone();
                    item.id = i64::from(id);
                    item.file_id = quill::ids::FileId(id);
                    item.format = if i % 2 == 0 {
                        quill::telegram::envelope::StickerFormat::Tgs
                    } else {
                        quill::telegram::envelope::StickerFormat::Webm
                    };
                    stickers.push(item);
                }
            }
            session.media_library.set_stickers.insert(77, stickers);
        }
        self.pickers.media_panel.open = true;
        self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Stickers;
        self.connection.status_note = "screenshot demo — stickers · tap to send".into();
    }

    fn demo_gifs(&mut self, demo: GifsDemo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_gifs(session, &self.demo_ui.sink, &self.demo_ui.seq);
            if demo == GifsDemo::GifPlayback {
                session.gifs.open = false;
                if let Some(history) = session.histories.get_mut(&11) {
                    history.messages.retain(|id, _| *id == 501);
                }
            }
        }
        if demo == GifsDemo::Gifs {
            self.pickers.media_panel.open = true;
            self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Gifs;
            self.toggle_animation_playback(
                MessageId(501),
                quill::ids::FileId(63),
                "image/gif".into(),
                cx,
            );
        }
    }
}

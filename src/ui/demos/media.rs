//! Screenshot demos: media.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::audio_playback::{apply_ready_audio, apply_ready_voice};
use crate::ui::composer_ui::apply_ready_stickers;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::demo::{
    seed_ready_custom_emoji_session, seed_ready_downloads_session, seed_ready_media_session,
};
use crate::ui::demos::video_fixtures::{
    apply_ready_gifs, apply_ready_video, apply_ready_video_note, apply_ready_video_viewer,
};
use crate::ui::play_intent::{PlayIntent, PlayTarget};
use crate::ui::playback::PlaybackKind;
use crate::ui::playback_state::{SeekHover, SeekSite};
use crate::ui::recording_state::{CaptureFor, CaptureMedia};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
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
    // A chat with an autoplaying video tile (204, the 12 s clip) and a
    // round video message (611) on the inline players: the video player
    // measurements (`docs/decisions/video-player-inline.md`) and captures.
    DemoSpec::chats(
        "ready-video-inline",
        "screenshot demo — inline video tile and round video autoplay"
    )
    .setup(QuillApp::demo_ready_video_inline),
    // Voice record bar + history playback (injected, no live Telegram).
    DemoSpec::chats(
        "ready-voice",
        "screenshot demo — voice record bar + history playback"
    )
    .setup(QuillApp::demo_ready_voice),
    // The incoming voice note (90) and the round video (611) not downloaded
    // yet: a press downloads and plays, a second press cancels.
    DemoSpec::chats(
        "ready-media-unloaded",
        "screenshot demo — voice note and round video before download"
    )
    .setup(QuillApp::demo_ready_media_unloaded),
    // The incoming voice note playing at ~40%: the waveform scrubber on
    // the active row and the player bar. `QUILL_DEMO_SEEK_HOVER=<0..1>`
    // puts the pointer over the row's scrubber.
    DemoSpec::chats(
        "ready-voice-playing",
        "screenshot demo — playing voice note with the waveform scrubber"
    )
    .setup(QuillApp::demo_ready_voice_playing),
    // Record tapped, camera not up yet: the circle answers with
    // "Starting camera…" (`recording.starting`).
    DemoSpec::chats(
        "ready-starting-camera",
        "screenshot demo — round video circle while the camera starts"
    )
    .setup(QuillApp::demo_starting_camera),
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
    fn demo_ready_audio(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_audio(session, &self.demo_ui.sink, &self.demo_ui.seq);
            // The song's right-click menu leads with the reaction strip.
            super::reaction_fixtures::seed_reaction_options(session, ChatId(11), MessageId(801));
        }
        // The demo has no audio device: the engine's explicit simulation plays
        // the track (`QUILL_DEMO_AUDIO_FAIL=1` shows a failed start instead,
        // `QUILL_DEMO_AUDIO_STALL=1` makes every seek hang).
        let track = demo_media_allowlist().join("demo-voice.ogg");
        self.begin_track_playback(
            PlaybackKind::Audio,
            ChatId(11),
            MessageId(801),
            214.0,
            87.0,
            &track,
            cx,
        );
        self.demo_seek_hover();
        self.connection
            .set_note(StatusNote::silent("screenshot demo — audio"));
    }

    fn demo_ready_voice_playing(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_voice(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // The incoming 12 s note (message 90), 40% in.
        let track = demo_media_allowlist().join("demo-voice.ogg");
        self.begin_track_playback(
            PlaybackKind::Voice,
            ChatId(11),
            MessageId(90),
            12.0,
            4.8,
            &track,
            cx,
        );
        self.demo_seek_hover();
        self.connection
            .set_note(StatusNote::silent("screenshot demo — voice note · playing"));
    }

    /// `QUILL_DEMO_SEEK_HOVER=<0..1>`: the pointer over the active row's
    /// scrubber, at that fraction.
    fn demo_seek_hover(&mut self) {
        self.playback.seek_hover = std::env::var("QUILL_DEMO_SEEK_HOVER")
            .ok()
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|at| at.is_finite())
            .map(|at| SeekHover {
                site: SeekSite::Row,
                at: at.clamp(0.0, 1.0),
            });
    }

    fn demo_ready_downloads(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Exercise failed playback recovery without a network request.
        let audio = |message, file| PlayIntent {
            file: FileId(file),
            target: PlayTarget::Audio {
                chat: ChatId(11),
                message: MessageId(message),
                duration: 1.,
            },
        };
        self.playback.intent = Some(audio(205, 26));
        self.discard_stopped_media_playback(cx);
        assert!(self.playback.intent.is_none());
        self.playback.intent = Some(audio(204, 24));
        self.discard_stopped_media_playback(cx);
        assert!(self.playback.intent.is_some());
        self.playback.intent = None;
    }

    fn demo_ready_emoji_packs(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_gifs(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.stickers.gifs.open = false;
            session.stickers.emoji.open = true;
            let pack = |id: i64, title: &str, emojis: &[&str], installed: bool| {
                quill::telegram::envelope::StickerSetInfo {
                    id,
                    title: title.into(),
                    name: title.into(),
                    size: emojis.len() as i32 * 12,
                    is_installed: installed,
                    is_official: false,
                    is_owned: false,
                    covers: emojis
                        .iter()
                        .enumerate()
                        .map(|(index, emoji)| quill::telegram::envelope::StickerItem {
                            custom_emoji_id: Some(id * 10 + index as i64),
                            id: id * 10 + index as i64,
                            set_id: id,
                            emoji: (*emoji).into(),
                            width: 100,
                            height: 100,
                            format: quill::telegram::envelope::StickerFormat::Webp,
                            file_id: quill::ids::FileId(0),
                            thumb_file_id: None,
                            thumb_width: 0,
                            thumb_height: 0,
                            requires_premium: false,
                            premium_animation: None,
                        })
                        .collect(),
                    cover_files: Vec::new(),
                    is_viewed: true,
                }
            };
            session.stickers.emoji.installed_sets = vec![
                pack(1, "Fun faces", &["😀", "😎", "🤩", "🥳"], true),
                pack(2, "Animals", &["🐶", "🐱", "🦊", "🐼"], true),
                pack(3, "Food and drink", &["🍕", "🍣", "🍩", "☕"], true),
            ];
            session.stickers.emoji.trending_sets = vec![
                pack(4, "Travel", &["✈️", "🏝️", "🗺️", "🚆"], false),
                pack(5, "Weather", &["☀️", "🌧️", "⛄", "🌈"], false),
                pack(2, "Animals", &["🐶", "🐱", "🦊", "🐼"], true),
            ];
            session.settings.media_prefs.recent_emoji_packs = vec![2, 1, 3];
        }
    }

    fn demo_ready_video(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — video · playing"));
    }

    fn demo_ready_video_note(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_note(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — video note · playing"));
    }

    fn demo_ready_video_inline(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_viewer(session, &self.demo_ui.sink, &self.demo_ui.seq);
            apply_ready_video_note(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — inline video autoplay",
        ));
    }

    fn demo_ready_media_unloaded(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_voice(session, &self.demo_ui.sink, &self.demo_ui.seq);
        apply_ready_video_note(session, &self.demo_ui.sink, &self.demo_ui.seq);
        session.settings.media_prefs.autoplay_videos = false;
        for file in [81, 92] {
            session.media.files.update(file, |file| {
                file.local.path.clear();
                file.local.is_downloading_completed = false;
            });
        }
    }

    fn demo_starting_camera(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.recording.starting = true;
        cx.notify();
    }

    fn demo_ready_voice(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_voice(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // One Play request must survive the download and start the note.
        if let Some(local) = self
            .demo_session
            .as_ref()
            .and_then(|session| session.media.files.get(&82))
            .cloned()
        {
            let mut waiting = local.clone();
            waiting.local.path.clear();
            waiting.local.is_downloading_completed = false;
            if let Some(session) = self.demo_session.as_mut() {
                session.media.files.insert(waiting);
            }
            self.toggle_voice_playback(ChatId(11), MessageId(91), FileId(82), true, 3., cx);
            assert!(self.playback.intent.is_some());
            if let Some(session) = self.demo_session.as_mut() {
                session.media.files.insert(local);
            }
            self.resume_play_intent(cx);
            assert!(
                self.playback.intent.is_none()
                    && self.playback.playing_voice == Some(MessageId(91))
            );
            self.stop_voice_playback();
        }
        let bars = vec![4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14];
        self.hold_capture(
            CaptureMedia::Voice(VoiceCapture::preview(
                demo_media_allowlist().join("demo-voice.ogg"),
                2,
                bars,
            )),
            CaptureFor::Message,
        );
        // MED2: demo shows the locked record bar + a transcribed note.
        self.recording.locked = true;
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — recording voice · locked",
        ));
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
            let mut stickers = session.stickers.stickers.stickers.clone();
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
                    let Some(mut item) = stickers.first().cloned() else {
                        break;
                    };
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
            session
                .media
                .media_library
                .set_stickers
                .insert(77, stickers);
        }
        self.pickers.media_panel.open = true;
        self.pickers.media_panel.tab = crate::ui::media_panel::PanelTab::Stickers;
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — stickers · tap to send",
        ));
    }

    fn demo_gifs(&mut self, demo: GifsDemo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_gifs(session, &self.demo_ui.sink, &self.demo_ui.seq);
            if demo == GifsDemo::GifPlayback {
                session.stickers.gifs.open = false;
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

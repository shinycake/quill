use super::AUDIO_BUILT;
use crate::ui::app::QuillApp;
use crate::ui::audio::engine::test_support::{device, track};
use crate::ui::audio::{AudioEngine, EngineState};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};

const DEMO_CHAT: ChatId = ChatId(11);
const DEMO_SONG: MessageId = MessageId(801);
const DEMO_SONG_SECS: f64 = 214.0;

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-audio-start-ui"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-audio")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

/// The real build path, held open: the row shows Pause at the asked
/// position, and nothing on it changes however long the file takes.
#[gpui_kit::test]
fn a_long_start_shows_pause_alone_on_the_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let (output, _device) = device();
    let song = track("ui-long-start.wav", 4);
    let (open, clock) = app.update(&mut vcx, |app, _| {
        let clock;
        (app.players.audio, clock) = AudioEngine::new(output).with_manual_clock();
        app.players.audio.set_simulation(None);
        let open = app.players.audio.gate_builds();
        app.players
            .audio
            .start(&song, 2.0, 1.0, 1.0, 214.0)
            .unwrap();
        (open, clock)
    });
    let shown = |app: &QuillApp| {
        let row = app.seek_bar_view(DEMO_CHAT, DEMO_SONG, DEMO_SONG_SECS);
        assert!(row.is_playing, "tdesktop: Pause from the press on");
        assert_eq!(row.display_secs, 2.0);
        app.track_shown((false, false))
    };
    let pressed = app.read_with(&vcx, |app, _| shown(app));
    clock.store(5000, std::sync::atomic::Ordering::Relaxed);
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.players.audio.state(), EngineState::Loading);
        assert_eq!(shown(app), pressed);
    });
    drop(open);
}

/// The poke stands in for the build thread, which can't touch a test
/// scheduler.
#[gpui_kit::test]
fn a_finished_build_lands_when_it_says_so_and_a_failed_one_tells_the_user(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let (output, _device) = device();
    let missing = track("ui-landing.wav", 1).with_file_name("ui-landing-missing.wav");
    app.update(&mut vcx, |app, cx| {
        app.players.audio = AudioEngine::new(output);
        app.players.audio.set_simulation(None);
        app.watch_audio_builds(cx);
        app.players.audio.set_on_built(|| {});
        app.players
            .audio
            .start(&missing, 1.0, 1.0, 1.0, 214.0)
            .unwrap();
        app.players.audio.await_build_done();
        assert_eq!(app.players.audio.state(), EngineState::Loading);
    });
    AUDIO_BUILT.poke();
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.players.audio.state(), EngineState::Failed);
        assert_eq!(
            app.connection.status_note(),
            tr(keys::Q_AV_AUDIO_CANT_BE_PLAYED)
        );
    });
}

/// Message ids repeat between chats: a position saved for message 801 of
/// one chat is not the seek position of the same id in another.
#[gpui_kit::test]
fn a_saved_position_belongs_to_its_chat(cx: &mut TestAppContext) {
    use crate::ui::playback_state::ChatMessage;
    let (app, mut vcx) = open_app(cx);
    let song = MessageId(DEMO_SONG.0 + 1000);
    let other = ChatId(DEMO_CHAT.0 + 1);
    app.update(&mut vcx, |app, _| {
        app.playback
            .positions
            .insert(ChatMessage::new(DEMO_CHAT, song), 90.0);
        let here = app.seek_bar_view(DEMO_CHAT, song, DEMO_SONG_SECS);
        let there = app.seek_bar_view(other, song, DEMO_SONG_SECS);
        assert_eq!(here.display_secs, 90.0);
        assert_eq!(there.display_secs, 0.0);
    });
}

/// A drag on the seek slider of a track that is no longer the active one
/// (the track changed under the press) neither previews nor seeks.
#[gpui_kit::test]
fn a_slider_of_the_previous_track_does_not_seek_the_new_one(cx: &mut TestAppContext) {
    use crate::ui::playback::PlaybackKind;
    use gpui_kit::component::slider::{SliderEvent, SliderValue};
    let (app, mut vcx) = open_app(cx);
    let song = track("ui-slider-owner.wav", 4);
    let begin = |app: &mut QuillApp, message: MessageId, cx: &mut gpui_kit::Context<QuillApp>| {
        app.begin_track_playback(
            PlaybackKind::Audio,
            DEMO_CHAT,
            message,
            DEMO_SONG_SECS,
            0.0,
            &song,
            cx,
        );
        app.playback.seek_slider.clone().expect("its slider")
    };
    let old = app.update(&mut vcx, |app, cx| begin(app, DEMO_SONG, cx));
    let current = app.update(&mut vcx, |app, cx| {
        begin(app, MessageId(DEMO_SONG.0 + 1), cx)
    });
    old.update(&mut vcx, |_, cx| {
        cx.emit(SliderEvent::Change(SliderValue::Single(100.0)));
    });
    app.read_with(&vcx, |app, _| {
        assert!(!app.playback.seek_scrubbing);
        assert_eq!(app.playback.seek_preview_secs, None);
    });
    old.update(&mut vcx, |_, cx| {
        cx.emit(SliderEvent::Release(SliderValue::Single(100.0)));
    });
    app.read_with(&vcx, |app, _| {
        assert!(app.players.audio.position_secs() < 50.0);
        assert_eq!(
            app.playback
                .positions
                .get(&crate::ui::playback_state::ChatMessage::new(
                    DEMO_CHAT,
                    MessageId(DEMO_SONG.0 + 1)
                )),
            None
        );
    });
    current.update(&mut vcx, |_, cx| {
        cx.emit(SliderEvent::Change(SliderValue::Single(100.0)));
    });
    app.read_with(&vcx, |app, _| {
        assert!(app.playback.seek_scrubbing);
        assert_eq!(app.playback.seek_preview_secs, Some(100.0));
    });
}

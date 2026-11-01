use super::AUDIO_BUILT;
use crate::ui::app::QuillApp;
use crate::ui::audio::engine::test_support::{device, track};
use crate::ui::audio::{AudioEngine, EngineState, Simulation};
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

fn is_showing_pause(app: &QuillApp) -> bool {
    app.seek_bar_view(DEMO_CHAT, DEMO_SONG, DEMO_SONG_SECS).is_playing
}

fn shows_loading(app: &QuillApp) -> bool {
    app.seek_bar_view(DEMO_CHAT, DEMO_SONG, DEMO_SONG_SECS).loading
}

#[gpui_kit::test]
fn a_start_that_takes_long_shows_pause_and_loading_on_the_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.read_with(&vcx, |app, _| {
        assert!(is_showing_pause(app));
        assert!(!shows_loading(app));
    });
    app.update(&mut vcx, |app, _| {
        app.playback.audio.set_simulation(Some(Simulation {
            fail: false,
            stall: false,
            slow: true,
        }));
    });
    app.read_with(&vcx, |app, _| {
        assert!(is_showing_pause(app), "tdesktop: Pause from the press on");
        assert!(shows_loading(app));
    });
}

/// The poke stands in for the build thread, which can't touch a test
/// scheduler.
#[gpui_kit::test]
fn a_finished_build_lands_when_it_says_so_and_a_failed_one_tells_the_user(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let (output, _device) = device();
    let missing = track("ui-landing.wav", 1).with_file_name("ui-landing-missing.wav");
    app.update(&mut vcx, |app, cx| {
        app.playback.audio = AudioEngine::new(output);
        app.playback.audio.set_simulation(None);
        app.watch_audio_builds(cx);
        app.playback.audio.set_on_built(|| {});
        app.playback
            .audio
            .start(&missing, 1.0, 1.0, 1.0, 214.0)
            .unwrap();
        app.playback.audio.await_build_done();
        assert_eq!(app.playback.audio.state(), EngineState::Loading);
    });
    AUDIO_BUILT.poke();
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.playback.audio.state(), EngineState::Failed);
        assert_eq!(
            app.connection.status_note(),
            tr(keys::Q_AV_AUDIO_CANT_BE_PLAYED)
        );
    });
}

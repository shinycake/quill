//! Screenshot demos of the agent takeover over the ready chat list, with
//! the takeover clock pinned so the glow and the ghost cursor hold still:
//!
//! - `ready-agent-driving`: driving, the ghost just clicked a chat row.
//! - `ready-agent-steady`: driving with nothing in motion (for measuring).
//! - `ready-agent-glide`: the ghost halfway from that row to the message box.
//! - `ready-agent-typing`: the keyboard badge at the message box's caret.
//! - `ready-agent-paused`: the user took control.
//!
//! `QUILL_AGENT_GLOW=edge|breathe|sweep` picks the glow, `QUILL_DEMO_THEME`
//! the theme. `QUILL_DEMO_TAKEOVER_FRAMES=<dir>` (demo-capture builds) makes
//! `ready-agent-driving` record a scripted session instead, one PNG per
//! 1/30 s of takeover time (`frame-000.png`...), to assemble into a clip.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::ui_driver::takeover::{self, Aim, PauseReason};
use quill::agent::{access, activity};

/// Where the session starts on the pinned clock.
const T0: u64 = 10_000;
/// The chat list's second row and the message box, at the demo's
/// 1100x720 window.
const ROW: (f32, f32) = (150.0, 272.0);
const MESSAGE_BOX: (f32, f32) = (520.0, 666.0);

fn at(ms: u64) {
    takeover::pin_clock(Some(T0 + ms));
}

/// Access and the driver on, one agent connected, a session started.
fn start_session() {
    quill::fast_buttons::set_persistence(false);
    access::set_enabled(true);
    access::set_ui_driver(true);
    // The demo window lives until the process ends.
    std::mem::forget(activity::connect("Claude Code"));
    at(0);
    takeover::note_command("Claude Code");
}

/// The ghost clicked the row at 500 ms; held 130 ms into the press pulse.
fn demo_driving(_: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
    start_session();
    if let Some(dir) = std::env::var_os("QUILL_DEMO_TAKEOVER_FRAMES") {
        record(std::path::PathBuf::from(dir), window, cx);
        return;
    }
    at(500);
    takeover::note_aim(Aim::Click, ROW);
    at(760);
    takeover::note_press();
    at(890);
}

/// Driving with nothing in motion (faded in, the click's pulse over): what
/// a steady takeover costs per glow look.
fn demo_steady(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    start_session();
    at(500);
    takeover::note_aim(Aim::Click, ROW);
    at(760);
    takeover::note_press();
    at(2000);
}

fn demo_glide(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    start_session();
    at(500);
    takeover::note_aim(Aim::Click, ROW);
    at(2000);
    takeover::note_aim(Aim::Click, MESSAGE_BOX);
    at(2000 + takeover::GLIDE.as_millis() as u64 * 2 / 5);
}

fn demo_paused(_: &mut QuillApp, _: &mut Window, _: &mut Context<QuillApp>) {
    start_session();
    at(500);
    takeover::note_aim(Aim::Click, ROW);
    at(3000);
    takeover::pause(PauseReason::Input);
}

/// Typing in the message box: the badge goes where the caret is once the
/// box has the focus and a frame laid it out.
fn demo_typing(app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
    start_session();
    app.composer.update(cx, |input, cx| {
        input.set_value("Lunch at noon works for me", window, cx);
        input.focus(window, cx);
        let end = input.value().len();
        input.set_selected_range(end..end, cx);
    });
    QuillApp::spawn_main_in(window, cx, async move |app, cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(600))
            .await;
        let _ = app.update_in(cx, |_, window, cx| {
            if let Some(caret) = window.focused_text_caret(cx) {
                at(3000);
                let right = caret.origin.x + caret.size.width;
                takeover::note_aim(Aim::Type, (right.as_f32(), caret.origin.y.as_f32()));
                at(3000 + takeover::GLIDE.as_millis() as u64);
            }
            window.refresh();
        });
    })
    .detach();
}

/// One step of the recorded session: what happens at a moment of it.
#[cfg(feature = "demo-capture")]
#[derive(Clone, Copy)]
enum Beat {
    Aim(Aim, (f32, f32)),
    Press,
    Stop,
}

/// The recorded session: fade in, a click on a row, a glide to the message
/// box and a click there, a steady stretch (a full breath or lap of the
/// sweep), then Stop and the fade out.
#[cfg(feature = "demo-capture")]
const SCRIPT: [(u64, Beat); 5] = [
    (500, Beat::Aim(Aim::Click, ROW)),
    (800, Beat::Press),
    (1300, Beat::Aim(Aim::Click, MESSAGE_BOX)),
    (1600, Beat::Press),
    (4000, Beat::Stop),
];
#[cfg(feature = "demo-capture")]
const RECORD_END_MS: u64 = 4700;
#[cfg(feature = "demo-capture")]
const FRAME_MS: u64 = 33;

#[cfg(feature = "demo-capture")]
fn record(dir: std::path::PathBuf, window: &mut Window, cx: &mut Context<QuillApp>) {
    let _ = std::fs::create_dir_all(&dir);
    QuillApp::spawn_main_in(window, cx, async move |_app, cx| {
        // Let the window open and draw before the first frame.
        cx.background_executor()
            .timer(std::time::Duration::from_millis(800))
            .await;
        let mut next_beat = 0usize;
        let mut frame = 0u32;
        let mut time = 0u64;
        while time <= RECORD_END_MS {
            while let Some((beat_at, beat)) = SCRIPT.get(next_beat).copied()
                && beat_at <= time
            {
                at(beat_at);
                match beat {
                    Beat::Aim(what, to) => takeover::note_aim(what, to),
                    Beat::Press => takeover::note_press(),
                    Beat::Stop => takeover::stop_driving(),
                }
                next_beat += 1;
            }
            at(time);
            let path = dir.join(format!("frame-{frame:03}.png"));
            // The window draws the app, so it runs outside the app's update.
            let saved = cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                window
                    .render_to_image()
                    .map_err(|err| err.to_string())
                    .and_then(|image| image.save(&path).map_err(|err| err.to_string()))
            });
            if let Ok(Err(err)) = saved {
                eprintln!("quill takeover recording: frame {frame}: {err}");
            }
            frame += 1;
            time += FRAME_MS;
        }
        let _ = std::fs::write(dir.join("done"), b"ok\n");
    })
    .detach();
}

#[cfg(not(feature = "demo-capture"))]
fn record(_: std::path::PathBuf, _: &mut Window, _: &mut Context<QuillApp>) {}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-driving").setup(demo_driving),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-steady").setup(demo_steady),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-glide").setup(demo_glide),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-paused").setup(demo_paused),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-agent-typing")
        .setup(demo_typing)
        .timing(2500, 3500),
];

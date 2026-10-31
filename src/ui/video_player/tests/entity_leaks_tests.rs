//! The media viewer playing a clip, and Picture-in-Picture, let go of what
//! they made when they close (the harness is `ui::esc_stack::entity_leaks_tests`).
use super::{Script, queue_backend};
use crate::ui::esc_stack::entity_leaks_tests::{Harness, Surface, check, new_app};
use gpui_kit::TestAppContext;
use quill::ids::{ChatId, MessageId};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;

/// The demo's clip in chat A.
const CLIP: MessageId = MessageId(204);

/// The viewer open on the clip, played by a fresh scripted backend.
fn open_clip(h: &mut Harness) {
    queue_backend(&Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    })));
    h.act(|app, _, cx| app.open_media_viewer(ChatId(11), CLIP, cx));
}

#[gpui_kit::test]
fn closing_a_playing_clip_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-privacy");
    h.act(|app, window, cx| {
        window.activate_window();
        if let Some(session) = app.demo_session.as_mut() {
            app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::demos::video_fixtures::apply_ready_video_viewer(
                session,
                &app.demo_ui.sink,
                &app.demo_ui.seq,
            );
        }
        cx.notify();
    });
    h.settle();
    let failures = check(
        &mut h,
        &[
            Surface {
                name: "media viewer playing a clip",
                open: open_clip,
                close: |h| h.act(|app, _, cx| app.close_media_viewer(cx)),
            },
            Surface {
                name: "Picture-in-Picture",
                open: |h| {
                    open_clip(h);
                    h.settle();
                    h.act(|app, _, cx| app.open_video_pip(cx));
                },
                close: |h| h.act(|app, _, cx| app.close_video_pip(cx)),
            },
        ],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

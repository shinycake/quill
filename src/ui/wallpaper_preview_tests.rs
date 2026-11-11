//! Apply closes the wallpaper box at once (tdesktop's `applyForEveryone`),
//! and the server's answer still flips the appearance afterwards.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::wallpaper_preview::PreviewSource;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::state::BackgroundOutcome;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    quill::lang::isolate_thread();
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn answer_apply(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, request: RequestId, ok: bool) {
    app.update_in(vcx, |app, _, cx| {
        app.dialogs.wallpaper_applies.push(request);
        if let Some(session) = app.demo_session.as_deref_mut() {
            let answer = if ok {
                Ok(BackgroundOutcome::Applied)
            } else {
                Err("FLOOD_WAIT".to_string())
            };
            session.chats_state.background_answers.push(request, answer);
        }
        app.drain_wallpaper_preview(cx);
    });
}

#[gpui_kit::test]
fn an_apply_answer_flips_the_appearance_after_the_box_closed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.set_appearance(cx, |a| a.telegram_wallpaper = false);
        app.open_wallpaper_preview(PreviewSource::File("/tmp/a.jpg".into()), cx);
        app.dialogs.wallpaper_applies.push(RequestId(7));
        app.close_wallpaper_preview(cx);
    });
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.wallpaper_preview.is_none());
        assert!(!app.appearance.telegram_wallpaper);
    });
    answer_apply(&app, vcx, RequestId(7), true);
    app.read_with(vcx, |app, _| {
        assert!(app.appearance.telegram_wallpaper, "answer after close");
        assert!(app.dialogs.wallpaper_applies.is_empty());
    });
}

#[gpui_kit::test]
fn a_refused_apply_leaves_the_appearance_and_is_forgotten(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.set_appearance(cx, |a| a.telegram_wallpaper = false);
    });
    answer_apply(&app, vcx, RequestId(9), false);
    app.read_with(vcx, |app, _| {
        assert!(!app.appearance.telegram_wallpaper);
        assert!(app.dialogs.wallpaper_applies.is_empty());
    });
}

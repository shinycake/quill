//! Apply closes the wallpaper box at once (tdesktop's `applyForEveryone`),
//! and the server's answer still flips the appearance afterwards.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::wallpaper_preview::{PreviewSource, PreviewStage};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::settings::WallpaperSource;
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
        app.set_appearance(cx, |a| a.wallpaper = WallpaperSource::Theme);
        app.open_wallpaper_preview(PreviewSource::File("/tmp/a.jpg".into()), cx);
        app.dialogs.wallpaper_applies.push(RequestId(7));
        app.close_wallpaper_preview(cx);
    });
    app.read_with(vcx, |app, _| {
        assert!(app.dialogs.wallpaper_preview.is_none());
        assert!(!app.appearance.wallpaper.is_telegram());
    });
    answer_apply(&app, vcx, RequestId(7), true);
    app.read_with(vcx, |app, _| {
        assert!(app.appearance.wallpaper.is_telegram(), "answer after close");
        assert!(app.dialogs.wallpaper_applies.is_empty());
    });
}

#[gpui_kit::test]
fn a_refused_apply_leaves_the_appearance_and_is_forgotten(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.set_appearance(cx, |a| a.wallpaper = WallpaperSource::Theme);
    });
    answer_apply(&app, vcx, RequestId(9), false);
    app.read_with(vcx, |app, _| {
        assert!(!app.appearance.wallpaper.is_telegram());
        assert!(app.dialogs.wallpaper_applies.is_empty());
    });
}

fn link_preview(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, request: u64) {
    app.update_in(vcx, |app, _, cx| {
        app.open_wallpaper_preview_at(
            PreviewSource::Link("bg".into()),
            PreviewStage::Looking {
                request: RequestId(request),
            },
            cx,
        );
    });
}

fn set_search(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    to: quill::state::BackgroundSearch,
) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.chats_state.background_search = to;
        }
        app.drain_wallpaper_preview(cx);
    });
}

fn stage(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<PreviewStage> {
    app.read_with(vcx, |app, _| {
        app.dialogs
            .wallpaper_preview
            .as_ref()
            .map(|p| p.stage.clone())
    })
}

#[gpui_kit::test]
fn a_link_lookup_is_answered_under_the_request_it_asked(cx: &mut TestAppContext) {
    use quill::state::BackgroundSearch;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    link_preview(&app, vcx, 5);
    set_search(&app, vcx, BackgroundSearch::Waiting(RequestId(5)));
    assert_eq!(
        stage(&app, vcx),
        Some(PreviewStage::Looking {
            request: RequestId(5)
        }),
        "its own search is still out"
    );
    set_search(
        &app,
        vcx,
        BackgroundSearch::Failed(RequestId(5), "NOT_FOUND".into()),
    );
    assert_eq!(
        stage(&app, vcx),
        Some(PreviewStage::Failed("NOT_FOUND".into()))
    );
    app.read_with(vcx, |app, _| {
        let search = app
            .demo_session
            .as_deref()
            .map(|s| s.chats_state.background_search.clone());
        assert_eq!(search, Some(BackgroundSearch::Idle));
    });
}

#[gpui_kit::test]
fn a_search_that_is_not_the_previews_fails_it_instead_of_leaving_it_looking(
    cx: &mut TestAppContext,
) {
    use quill::state::BackgroundSearch;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    link_preview(&app, vcx, 5);
    set_search(&app, vcx, BackgroundSearch::Waiting(RequestId(6)));
    assert_eq!(stage(&app, vcx), Some(PreviewStage::Failed(String::new())));
    link_preview(&app, vcx, 7);
    set_search(
        &app,
        vcx,
        BackgroundSearch::Failed(RequestId(5), "OLD".into()),
    );
    assert_eq!(stage(&app, vcx), Some(PreviewStage::Failed(String::new())));
}

#[gpui_kit::test]
fn changing_blur_drops_a_share_still_out(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_wallpaper_preview_at(
            PreviewSource::File("/tmp/a.jpg".into()),
            PreviewStage::Sharing {
                request: RequestId(3),
            },
            cx,
        );
        app.set_preview_blur(true);
        let preview = app.dialogs.wallpaper_preview.as_ref().expect("open");
        assert_eq!(preview.stage, PreviewStage::Editing);
        assert_eq!(preview.blur, Some(true));
    });
}

/// Cancel while Share is out: the link that arrives later belongs to no
/// box, and the next drain drops it instead of keeping it in the session.
#[gpui_kit::test]
fn a_share_link_arriving_after_cancel_is_dropped(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.open_wallpaper_preview_at(
            PreviewSource::File("/tmp/a.jpg".into()),
            PreviewStage::Sharing {
                request: RequestId(31),
            },
            cx,
        );
        app.close_wallpaper_preview(cx);
        if let Some(session) = app.demo_session.as_deref_mut() {
            session
                .chats_state
                .background_answers
                .push(RequestId(31), Ok(BackgroundOutcome::Link("t.me/bg".into())));
        }
        app.drain_wallpaper_preview(cx);
        let kept = app
            .demo_session
            .as_deref()
            .is_some_and(|session| session.chats_state.background_answers.is_empty());
        assert!(kept, "the late link was dropped, not kept");
    });
}

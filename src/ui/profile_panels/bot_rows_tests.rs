//! The bot and channel action rows of the info panel, through the
//! `ready-info-bot` and `ready-info-channel-left` demos.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(
    cx: &mut TestAppContext,
    kind: &'static str,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1200.), px(1400.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

/// The row's top edge in window points, `None` when it is not drawn.
fn top(vcx: &mut VisualTestContext, id: &'static str) -> Option<f32> {
    vcx.update(|window, _| window.try_find(id).map(|row| f32::from(row.bounds().origin.y)))
}

#[gpui_kit::test]
fn a_bots_panel_lists_help_settings_privacy_then_report_and_block(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_demo(cx, "ready-info-bot");
    let vcx = &mut vcx;
    let rows = [
        "info-panel-bot-help",
        "info-panel-bot-settings",
        "info-panel-bot-privacy",
        "info-panel-bot-report",
        "info-panel-block-user",
    ];
    let tops: Vec<Option<f32>> = rows.iter().map(|id| top(vcx, id)).collect();
    assert!(tops.iter().all(Option::is_some), "rows missing: {tops:?}");
    let ys: Vec<f32> = tops.into_iter().flatten().collect();
    assert!(ys.windows(2).all(|pair| pair[0] < pair[1]), "order: {ys:?}");
    assert!(top(vcx, "info-panel-channel-join").is_none());
}

#[gpui_kit::test]
fn join_shows_on_an_unjoined_channel_and_goes_once_joined(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-info-channel-left");
    let vcx = &mut vcx;
    let join = top(vcx, "info-panel-channel-join");
    let report = top(vcx, "info-panel-channel-report");
    assert!(
        matches!((join, report), (Some(join), Some(report)) if join < report),
        "join {join:?} report {report:?}"
    );
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| app.join_channel(ChatId(73), cx));
    });
    frames(vcx);
    assert!(top(vcx, "info-panel-channel-join").is_none());
}

//! UI tests for the revenue and affiliate pages (needs the `demo-capture`
//! feature for gpui-kit's test support). They run on the screenshot demos'
//! injected sessions: nothing connects, and every action that would talk to
//! Telegram ends in the demo-mode note instead.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px,
    size,
};
use quill::earn::{LinkBox, RevokeBox, SetupConfirm};

const DEMO_NOTE: &str = "Demo mode: this needs a live session.";

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
    }
}

/// A window on one of the earn screenshot demos (its setup opens the page).
fn open_demo(
    cx: &mut TestAppContext,
    kind: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    // Tall enough that a whole page is on screen.
    let handle = cx.open_window(size(px(1200.), px(2400.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(kind))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let Some((app, shell)) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

/// Click the middle of `selector` once it stops moving: a kit dialog
/// scales in on the real clock, and a click taken mid-animation can
/// land beside the button on a busy machine.
fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let mut at = bounds(vcx, selector);
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        frames(vcx);
        let now = bounds(vcx, selector);
        if now == at {
            break;
        }
        at = now;
    }
    let at = at.center();
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
}

fn open_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn note(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, _| app.connection.status_note().to_string())
}

#[gpui_kit::test]
fn the_monetization_page_scrolls_to_collect_which_needs_a_live_session(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-earn-monetization");
    let vcx = &mut vcx;
    let panel = bounds(vcx, "earn-panel");
    assert!(panel.size.width > px(200.), "the page fills the info panel");
    // The balances sit below the charts; scroll there.
    app.update(vcx, |app, cx| {
        app.earn.scroll.set_offset(point(px(0.), px(-100_000.)));
        cx.notify();
    });
    frames(vcx);
    click(vcx, "earn-collect");
    // Collect starts a Fragment withdrawal; the demo has no session for it.
    assert_eq!(note(&app, vcx), DEMO_NOTE);
}

#[gpui_kit::test]
fn the_learn_box_counts_diamonds_and_closes(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-learn");
    let vcx = &mut vcx;
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnLearn));
    for _ in 0..3 {
        click(vcx, "earn-learn-diamond");
    }
    app.read_with(vcx, |app, _| assert_eq!(app.earn.diamonds, 3));
    assert!(!note(&app, vcx).is_empty(), "each diamond shows a toast");
    click(vcx, "earn-learn-close");
    app.read_with(vcx, |app, _| assert!(!app.earn.learn));
    assert_ne!(open_dialog(&shell, vcx), Some(DialogKind::EarnLearn));
}

#[gpui_kit::test]
fn a_joined_program_opens_its_link_and_copy_closes_it(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-affiliate-join");
    let vcx = &mut vcx;
    // My Programs (joined) and the programs to join are both listed.
    for selector in [
        "earn-program-9102",
        "earn-program-9103",
        "earn-program-9104",
    ] {
        bounds(vcx, selector);
    }
    click(vcx, "earn-program-9102");
    app.read_with(vcx, |app, _| {
        let link = app.earn_state().and_then(|e| e.link_box.as_ref());
        assert_eq!(link.map(|l| l.program.bot_user_id), Some(9102));
    });
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnLink));
    click(vcx, "earn-link-copy");
    assert_eq!(note(&app, vcx), "Link copied to clipboard.");
    app.read_with(vcx, |app, _| {
        assert!(app.earn_state().is_some_and(|e| e.link_box.is_none()));
    });
    assert_ne!(open_dialog(&shell, vcx), Some(DialogKind::EarnLink));
    // A program not joined yet asks TDLib first: the demo has no session.
    click(vcx, "earn-program-9103");
    assert_eq!(note(&app, vcx), DEMO_NOTE);
}

#[gpui_kit::test]
fn leaving_asks_to_revoke_and_cancel_keeps_the_program(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-affiliate-join");
    let vcx = &mut vcx;
    // The row's "Leave" menu item opens this box.
    app.update(vcx, |app, cx| {
        let joined = app
            .earn_state()
            .and_then(|e| e.join.as_ref())
            .and_then(|j| j.mine.programs.first().cloned().map(|p| (p, j.peer)));
        let Some((program, recipient)) = joined else {
            panic!("the demo has a joined program");
        };
        if let Some(earn) = app.earn_mut() {
            earn.revoke = Some(RevokeBox {
                program,
                recipient,
                sending: false,
            });
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnRevoke));
    click(vcx, "earn-revoke-cancel");
    app.read_with(vcx, |app, _| {
        let earn = app.earn_state().expect("earn state");
        assert!(earn.revoke.is_none());
        assert_eq!(earn.join.as_ref().map(|j| j.mine.programs.len()), Some(1));
    });
    assert_ne!(open_dialog(&shell, vcx), Some(DialogKind::EarnRevoke));
}

#[gpui_kit::test]
fn the_join_box_asks_for_a_live_session(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-join-box");
    let vcx = &mut vcx;
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnJoin));
    click(vcx, "earn-join-program");
    assert_eq!(note(&app, vcx), DEMO_NOTE);
}

#[gpui_kit::test]
fn the_link_box_opens_from_the_demo(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-link-box");
    let vcx = &mut vcx;
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnLink));
    app.read_with(vcx, |app, _| {
        let link: Option<&LinkBox> = app.earn_state().and_then(|e| e.link_box.as_ref());
        assert!(link.is_some_and(|l| !l.program.url.is_empty()));
    });
}

#[gpui_kit::test]
fn setup_builds_its_sliders_and_save_asks_to_confirm(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-earn-affiliate-setup");
    let vcx = &mut vcx;
    app.read_with(vcx, |app, _| {
        let commission = app.earn.commission.as_ref().map(|(_, stops)| stops.len());
        assert!(commission.is_some_and(|n| n > 1), "commission stops");
        let duration = app.earn.duration.as_ref().map(|(_, stops)| stops.len());
        assert_eq!(duration, Some(7), "1m, 3m, 6m, 1y, 2y, 3y and forever");
    });
    click(vcx, "earn-setup-save");
    app.read_with(vcx, |app, _| {
        let confirm = app
            .earn_state()
            .and_then(|e| e.setup.as_ref())
            .and_then(|s| s.confirm);
        assert_eq!(confirm, Some(SetupConfirm::Save));
    });
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EarnSetupConfirm));
    click(vcx, "earn-setup-cancel");
    app.read_with(vcx, |app, _| {
        let confirm = app
            .earn_state()
            .and_then(|e| e.setup.as_ref())
            .and_then(|s| s.confirm);
        assert_eq!(confirm, None);
    });
    assert_ne!(open_dialog(&shell, vcx), Some(DialogKind::EarnSetupConfirm));
}

#[gpui_kit::test]
fn the_revenue_charts_draw_with_the_kit_charts(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = open_demo(cx, "ready-earn-monetization");
    let vcx = &mut vcx;
    let panel = bounds(vcx, "earn-panel");
    for id in [
        "earn-chart-hours-plot",
        "earn-chart-ton-plot",
        "earn-chart-stars-plot",
    ] {
        let chart = bounds(vcx, id);
        assert_eq!(
            chart.size.height,
            px(200.),
            "{id} has the tdesktop chart height"
        );
        assert!(
            chart.right() <= panel.right(),
            "{id} stays inside the panel"
        );
    }
}

#[gpui_kit::test]
fn the_bot_stars_page_draws_its_revenue_chart(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = open_demo(cx, "ready-earn-bot-stars");
    // tdesktop `ChartWidget`: the plot, then the range selector under it.
    let plot = bounds(&mut vcx, "earn-chart-bot-plot");
    assert_eq!(plot.size.height, px(200.));
    let range = bounds(&mut vcx, "earn-chart-bot-range");
    assert!(range.origin.y >= plot.origin.y + plot.size.height);
}

#[gpui_kit::test]
fn the_page_is_a_centered_layer_that_closes(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-earn-monetization");
    let vcx = &mut vcx;
    // tdesktop `Info::LayerWidget`: `infoDesiredWidth`, centered.
    let layer = bounds(vcx, "earn-layer");
    assert_eq!(layer.size.width, px(392.));
    assert_eq!(layer.center().x, px(600.));
    click(vcx, "earn-layer-close");
    frames(vcx);
    app.read_with(vcx, |app, _| assert!(!app.earn_layer_open()));
}

#[gpui_kit::test]
fn the_setup_button_stays_pinned_under_the_page(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = open_demo(cx, "ready-earn-affiliate-setup");
    let vcx = &mut vcx;
    let page = bounds(vcx, "earn-panel");
    let save = bounds(vcx, "earn-setup-save");
    // tdesktop `setupBottom`: below the scrolling part, not inside it.
    assert!(save.origin.y >= page.bottom());
}

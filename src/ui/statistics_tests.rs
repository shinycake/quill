//! UI tests for the statistics panel: every chart kind draws, the arrow
//! keys read a day out, the legend and range selector respond, a click on
//! a point zooms into a day or a pie, and a recent post opens its own page.

use super::fixture::{
    ZOOM_TOKEN, seed_channel_statistics, seed_message_statistics, seed_zoomed_day,
};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px,
    size,
};
use quill::state::{InfoPanelTarget, StatsPage};

const CHANNEL: i64 = 13;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_statistics(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    // Tall enough that the whole panel is on screen.
    let handle = cx.open_window(size(px(1200.), px(5000.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-channels")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        seed_channel_statistics(app.demo_session.as_mut().expect("demo"), CHANNEL);
        app.open_info_panel_target(InfoPanelTarget::Statistics(CHANNEL), window, cx);
    });
    frames(&mut vcx);
    (app, vcx)
}

/// Bounds of a debug selector (they must be `'static`; leaking a few
/// strings in a test is fine).
fn find(vcx: &mut VisualTestContext, selector: &str) -> Option<Bounds<Pixels>> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    vcx.debug_bounds(selector)
}

fn bounds(vcx: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    find(vcx, selector).unwrap_or_else(|| panic!("{selector} is drawn"))
}

#[gpui_kit::test]
fn every_chart_kind_draws_and_failed_graphs_stay_out(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    for id in [
        "growth",
        "followers",
        "mute",
        "hours",
        "view-sources",
        "languages",
        "interactions",
        "reactions",
        "story-interactions",
    ] {
        let plot = bounds(vcx, &format!("stats-{CHANNEL}-{id}-plot"));
        assert!(plot.size.width > px(200.), "{id} has the panel's width");
    }
    // The growth chart has one line: no legend. Two lines: a legend.
    assert!(find(vcx, &format!("stats-{CHANNEL}-growth-line-0")).is_none());
    assert!(find(vcx, &format!("stats-{CHANNEL}-followers-line-1")).is_some());
    // A graph the server could not build is left out.
    assert!(find(vcx, &format!("stats-{CHANNEL}-join-sources-plot")).is_none());
    assert!(find(vcx, &format!("stats-{CHANNEL}-iv-plot")).is_none());
}

/// Point at `at` so the plot's tooltip picks the point under it. A plot
/// shows no tooltip while its marks draw in, so run the clock past that
/// first, then let the tooltip fade in.
fn hover(vcx: &mut VisualTestContext, at: gpui_kit::Point<Pixels>) {
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    frames(vcx);
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    frames(vcx);
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    frames(vcx);
}

#[gpui_kit::test]
fn arrow_keys_read_out_a_point_and_the_legend_and_range_respond(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    let details = format!("stats-{CHANNEL}-followers-details");
    let plot = bounds(vcx, &format!("stats-{CHANNEL}-followers-plot"));
    assert!(find(vcx, &details).is_none(), "no point is picked yet");
    // A click focuses the plot; the arrow keys pick a day, Escape drops it.
    vcx.simulate_click(plot.center(), Modifiers::default());
    vcx.simulate_keystrokes("right");
    frames(vcx);
    assert!(
        find(vcx, &details).is_some(),
        "an arrow key reads a day out"
    );
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(find(vcx, &details).is_none(), "escape clears it");
    assert!(
        find(vcx, &format!("stats-{CHANNEL}-followers-plot")).is_some(),
        "escape leaves the panel open"
    );
    // Switching a line off keeps the chart drawing.
    let chip = bounds(vcx, &format!("stats-{CHANNEL}-followers-line-0"));
    vcx.simulate_click(chip.center(), Modifiers::default());
    frames(vcx);
    bounds(vcx, &format!("stats-{CHANNEL}-followers-plot"));
    // The range selector is drawn under the plot.
    let range = bounds(vcx, &format!("stats-{CHANNEL}-followers-range"));
    assert!(range.top() >= plot.bottom());
}

#[gpui_kit::test]
fn a_percentage_chart_zooms_into_a_pie_and_back(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    let zoom_out = format!("stats-{CHANNEL}-languages-pie-zoom-out");
    let plot = bounds(vcx, &format!("stats-{CHANNEL}-languages-plot"));
    let at = point(plot.right() - px(30.), plot.center().y);
    hover(vcx, at);
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
    assert!(find(vcx, &zoom_out).is_some(), "a click opens the pie");
    // Escape zooms back out.
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(find(vcx, &zoom_out).is_none(), "zoomed out");
    bounds(vcx, &format!("stats-{CHANNEL}-languages-plot"));
}

#[gpui_kit::test]
fn export_builds_the_page_workbook(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        let (sheets, name) =
            crate::ui::statistics_export::page_workbook(session, CHANNEL).expect("overview");
        let names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names[0], "Overview");
        // The async graph that loaded is exported too; error graphs not.
        assert!(names.contains(&"Notifications"));
        assert!(!names.contains(&"New followers by source"));
        assert!(name.ends_with(".xlsx"));
        assert!(!quill::stat_chart::serialize_workbook(&sheets).is_empty());
        seed_message_statistics(session, CHANNEL, CHANNEL);
        let (sheets, name) =
            crate::ui::statistics_export::page_workbook(session, CHANNEL).expect("message");
        assert_eq!(sheets[1].name, "Interactions");
        assert!(name.contains("_100_"), "{name}");
    });
}

#[gpui_kit::test]
fn a_click_on_a_point_zooms_into_the_day(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    let plot = bounds(vcx, &format!("stats-{CHANNEL}-growth-plot"));
    let at = point(plot.right() - px(20.), plot.center().y);
    hover(vcx, at);
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let stats = &app.session().unwrap().groups.stats;
        assert!(stats.zoomed.contains_key(ZOOM_TOKEN), "the click zooms");
    });
    // The day arrives: its hourly chart takes the place.
    let x = app.update(vcx, |app, cx| {
        let x = seed_zoomed_day(app.demo_session.as_mut().unwrap(), CHANNEL);
        cx.notify();
        x
    });
    frames(vcx);
    bounds(vcx, &format!("stats-{CHANNEL}-growth-day-{x}-plot"));
    app.update(vcx, |app, cx| app.unzoom_stat_graph(ZOOM_TOKEN, cx));
    frames(vcx);
    bounds(vcx, &format!("stats-{CHANNEL}-growth-plot"));
}

#[gpui_kit::test]
fn a_recent_post_opens_its_statistics_and_back_returns(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_statistics(cx);
    let vcx = &mut vcx;
    // Ten posts first; "Show more" lists the rest.
    assert!(find(vcx, "recent-post-9").is_some());
    assert!(find(vcx, "recent-post-10").is_none());
    let row = bounds(vcx, "recent-post-0");
    vcx.simulate_click(row.center(), Modifiers::default());
    frames(vcx);
    let message_id = app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().unwrap();
        assert_eq!(
            session.groups.stats.page(CHANNEL),
            StatsPage::Message(100 << 20)
        );
        let id = seed_message_statistics(session, CHANNEL, CHANNEL);
        cx.notify();
        id
    });
    frames(vcx);
    bounds(
        vcx,
        &format!("message-{CHANNEL}-{message_id}-interactions-plot"),
    );
    assert!(find(vcx, "message-forward-0").is_some());
    assert!(find(vcx, "message-forward-1").is_some());
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.groups.stats.set_page(CHANNEL, StatsPage::Overview);
        cx.notify();
    });
    frames(vcx);
    bounds(vcx, &format!("stats-{CHANNEL}-growth-plot"));
}

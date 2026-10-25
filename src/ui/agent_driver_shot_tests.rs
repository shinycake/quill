//! Screenshots in a real window: rows of ungranted chats are hidden in
//! the granted scope and shown with All chats on, a blocked surface gets
//! no screenshot, an input can bring one back and it is logged, and the
//! takeover layer is never in one. The test platform has no GPU, so the
//! frame is rasterized from its quads (`agent_driver_shot::raster`).
use super::tests::{click, grant, new_app, pump, refused, settle, turn_on};
use crate::ui::agent_driver_shot::{HIDDEN_FILL, raster};
use crate::ui::agent_state::TEST_LOCK;
use crate::ui::app::QuillApp;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Bounds, Entity, Pixels, Quad, Rgba, TestAppContext, VisualTestContext};
use quill::agent::grants::{self, Scope};
use quill::agent::ui_driver::bridge::{self, BridgeError, Target, UiCommand, UiReply};
use quill::agent::ui_driver::shot::{self, Shot, ShotOptions, ShotScope};
use quill::agent::ui_driver::takeover::{self, FADE_IN};
use quill::agent::ui_driver::tree::UiTree;
use quill::agent::{access, activity};
use quill::ids::AccountKey;
use std::time::Duration;

const FULL: ShotOptions = ShotOptions {
    full_resolution: true,
};

/// File `command` with a screenshot and pump the window until answered.
fn drive_shot(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    command: UiCommand,
    options: ShotOptions,
) -> Result<UiReply, BridgeError> {
    shot::reset_limits();
    let ticket = bridge::post_with_shot("Test agent", command, Some(options))?;
    for _ in 0..40 {
        pump(app, vcx);
        if let Some(reply) = ticket.poll() {
            return Ok(reply);
        }
    }
    ticket.wait(Duration::from_millis(10))
}

fn take(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    options: ShotOptions,
) -> (UiTree, Shot) {
    match drive_shot(app, vcx, UiCommand::ReadTree, options) {
        Ok(UiReply::Shot { tree, shot }) => (tree, shot),
        other => panic!("expected a screenshot, got {other:?}"),
    }
}

/// A chat row's bounds in device pixels.
fn row(vcx: &mut VisualTestContext, chat_id: u64) -> Bounds<f32> {
    vcx.update(|window, _| {
        let bounds = window.find(("chat-row", chat_id)).bounds();
        device(bounds, window.scale_factor())
    })
}

fn device(bounds: Bounds<Pixels>, scale: f32) -> Bounds<f32> {
    Bounds {
        origin: gpui_kit::point(
            bounds.origin.x.as_f32() * scale,
            bounds.origin.y.as_f32() * scale,
        ),
        size: gpui_kit::size(
            bounds.size.width.as_f32() * scale,
            bounds.size.height.as_f32() * scale,
        ),
    }
}

fn quad_bounds(quad: &Quad) -> Bounds<f32> {
    Bounds {
        origin: gpui_kit::point(quad.bounds.origin.x.0, quad.bounds.origin.y.0),
        size: gpui_kit::size(quad.bounds.size.width.0, quad.bounds.size.height.0),
    }
}

/// `inner` lies within `outer`, give or take a device pixel.
fn within(inner: Bounds<f32>, outer: Bounds<f32>) -> bool {
    inner.origin.x >= outer.origin.x - 1.0
        && inner.origin.y >= outer.origin.y - 1.0
        && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + 1.0
        && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + 1.0
}

fn same(a: Bounds<f32>, b: Bounds<f32>) -> bool {
    within(a, b) && within(b, a)
}

fn is_hidden_fill(quad: &Quad) -> bool {
    let [r, g, b] = HIDDEN_FILL.map(|c| f32::from(c) / 255.0);
    quad.background
        .as_solid()
        .map(Rgba::from)
        .is_some_and(|c| (c.r - r).abs() < 0.01 && (c.g - g).abs() < 0.01 && (c.b - b).abs() < 0.01)
}

/// The pixel at the centre of `bounds` in a full-resolution screenshot.
fn pixel_at(shot: &Shot, bounds: Bounds<f32>) -> [u8; 3] {
    let image = image::load_from_memory(&shot.png)
        .unwrap_or_else(|error| panic!("a PNG: {error}"))
        .to_rgb8();
    let x = (bounds.origin.x + bounds.size.width / 2.0) as u32;
    let y = (bounds.origin.y + bounds.size.height / 2.0) as u32;
    image
        .get_pixel_checked(x, y)
        .map(|pixel| pixel.0)
        .unwrap_or_else(|| panic!("({x}, {y}) in {}x{}", image.width(), image.height()))
}

#[gpui_kit::test]
fn ungranted_rows_are_hidden_and_all_chats_shows_them(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let (granted_row, ungranted_row) = (row(vcx, 11), row(vcx, 12));
    let normal = vcx.update(|window, _| window.painted_quads());
    assert!(
        normal
            .iter()
            .any(|q| within(quad_bounds(q), ungranted_row) && !is_hidden_fill(q)),
        "the normal frame paints chat B's row"
    );

    // The default size: the long side shrunk to 1280, the window's shape.
    let (tree, small) = take(&app, vcx, ShotOptions::default());
    let (device_width, device_height) = vcx.update(|window, _| {
        let size = window.viewport_size();
        let scale = window.scale_factor();
        (
            (size.width.as_f32() * scale).round() as u32,
            (size.height.as_f32() * scale).round() as u32,
        )
    });
    assert_eq!(
        (small.width, small.height),
        shot::output_size(device_width, device_height, false)
    );
    let decoded = image::load_from_memory(&small.png).unwrap_or_else(|e| panic!("a PNG: {e}"));
    assert_eq!(
        (decoded.width(), decoded.height()),
        (small.width, small.height)
    );
    assert!((small.scale - small.width as f32 / tree.width).abs() < 0.001);
    assert_eq!(small.scope, ShotScope::Granted);

    // Full resolution: chat B's row is one grey block, chat A's is drawn.
    let (_, full) = take(&app, vcx, FULL);
    assert_eq!((full.width, full.height), (device_width, device_height));
    let captured = raster::captured();
    let in_row = |bounds: Bounds<f32>| -> Vec<&Quad> {
        captured
            .iter()
            .filter(|q| within(quad_bounds(q), bounds))
            .collect()
    };
    // Nothing of chat B's row reaches the frame: the rows' container is
    // not the agent's either, so the grey the window starts with shows.
    let hidden = in_row(ungranted_row);
    assert!(
        hidden.iter().all(|q| is_hidden_fill(q)),
        "nothing of chat B's row: {hidden:?}"
    );
    assert_eq!(pixel_at(&full, ungranted_row), HIDDEN_FILL);
    assert!(
        in_row(granted_row).iter().any(|q| !is_hidden_fill(q)),
        "chat A's row is drawn"
    );
    assert_ne!(pixel_at(&full, granted_row), HIDDEN_FILL);

    // All chats on: chat B's row is drawn like any other.
    grants::update(&AccountKey::primary(), |g| g.all_chats = Some(Scope::Read));
    settle(vcx);
    let (_, everything) = take(&app, vcx, FULL);
    assert_eq!(everything.scope, ShotScope::AllChats);
    let captured = raster::captured();
    assert!(
        !captured.iter().any(is_hidden_fill),
        "nothing covered with All chats on"
    );
    assert!(
        captured
            .iter()
            .any(|q| within(quad_bounds(q), ungranted_row)),
        "chat B's row is drawn"
    );
    assert_ne!(pixel_at(&everything, ungranted_row), HIDDEN_FILL);
    grants::update(&AccountKey::primary(), |g| g.all_chats = None);

    // Each screenshot is in the activity log; the window shows the full
    // frame again after.
    let log = activity::recent(&AccountKey::primary());
    let shots = log.iter().filter(|a| a.tool == "ui_screenshot").count();
    assert_eq!(shots, 3, "{log:?}");
    assert!(
        log.iter()
            .any(|a| a.summary == "screenshot (full resolution)")
    );
    let after = vcx.update(|window, _| (window.has_paint_filter(), window.painted_quads()));
    assert!(!after.0);
    assert!(
        after
            .1
            .iter()
            .any(|q| within(quad_bounds(q), ungranted_row) && !is_hidden_fill(q))
    );
}

#[gpui_kit::test]
fn a_blocked_surface_gets_no_screenshot(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    vcx.update(|_, cx| {
        app.update(cx, |app, _| app.open_settings(None));
    });
    settle(vcx);
    let reply = drive_shot(&app, vcx, UiCommand::ReadTree, FULL);
    assert!(refused(&reply, "Quill is showing Settings"), "{reply:?}");
    let log = activity::recent(&AccountKey::primary());
    assert!(log.iter().all(|a| a.tool != "ui_screenshot"), "{log:?}");
    assert!(!vcx.update(|window, _| window.has_paint_filter()));

    // The user took control: the window is theirs, no screenshot.
    vcx.update(|_, cx| {
        app.update(cx, |app, _| app.close_settings());
    });
    settle(vcx);
    takeover::reset_for_test();
    takeover::pause(takeover::PauseReason::Input);
    let reply = drive_shot(&app, vcx, UiCommand::ReadTree, FULL);
    assert!(refused(&reply, "took control"), "{reply:?}");
    takeover::reset_for_test();

    // The driver switched off: no screenshot either.
    access::set_ui_driver(false);
    let reply = drive_shot(&app, vcx, UiCommand::ReadTree, FULL);
    assert!(!matches!(reply, Ok(UiReply::Shot { .. })), "{reply:?}");
}

#[gpui_kit::test]
fn an_input_brings_back_a_screenshot_of_the_frame_after(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let search = match drive_shot(&app, vcx, UiCommand::ReadTree, FULL) {
        Ok(UiReply::Shot { tree, .. }) => tree
            .nodes
            .iter()
            .find(|n| n.role.ends_with("Input") && n.bounds.is_some())
            .map(|n| n.id.clone())
            .unwrap_or_else(|| panic!("a text input")),
        other => panic!("expected a screenshot, got {other:?}"),
    };
    let reply = drive_shot(
        &app,
        vcx,
        click(Target::Node(search)),
        ShotOptions::default(),
    );
    let Ok(UiReply::Done {
        tree,
        shot: Some(Ok(shot)),
        refusals,
        ..
    }) = reply
    else {
        panic!("expected Done with a screenshot, got {reply:?}");
    };
    assert!(refusals.is_empty(), "{refusals:?}");
    assert!((shot.scale - shot.width as f32 / tree.width).abs() < 0.001);
    let log = activity::recent(&AccountKey::primary());
    let tools: Vec<&str> = log.iter().map(|a| a.tool.as_str()).collect();
    assert_eq!(
        tools,
        ["ui_screenshot", "ui_click", "ui_screenshot"],
        "{log:?}"
    );
    assert!(log.iter().all(|a| a.chat_id == 11 && a.ok));
}

#[gpui_kit::test]
fn the_takeover_layer_is_not_in_a_screenshot(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    takeover::reset_for_test();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    takeover::pin_clock(Some(1000));
    takeover::note_command("Test agent");
    takeover::pin_clock(Some(1000 + FADE_IN.as_millis() as u64));
    settle(vcx);
    let pill = |vcx: &mut VisualTestContext| {
        let bounds = vcx.debug_bounds("agent-takeover-pill");
        let scale = vcx.update(|window, _| window.scale_factor());
        bounds.map(|b| device(b, scale))
    };
    let shown = pill(vcx).unwrap_or_else(|| panic!("the pill shows"));
    let has_pill = |quads: &[Quad]| quads.iter().any(|q| same(quad_bounds(q), shown));
    assert!(has_pill(&vcx.update(|window, _| window.painted_quads())));

    let _ = take(&app, vcx, FULL);
    assert!(!has_pill(&raster::captured()), "no pill in the screenshot");
    assert!(
        has_pill(&vcx.update(|window, _| window.painted_quads())),
        "the window shows the pill again"
    );
    takeover::reset_for_test();
}

/// Two rows in an allowed region, for the paint filter alone.
struct Rows;

impl gpui_kit::Render for Rows {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{
            InteractiveElement, ParentElement, StatefulInteractiveElement, Styled, div, px, rgb,
        };
        let row = |tag: &'static str, color: u32| {
            div()
                .id(tag)
                .a11y_tag(tag)
                .w(px(100.))
                .h(px(40.))
                .bg(rgb(color))
                .child(div().size(px(10.)).bg(rgb(0x00_00ff)))
        };
        div()
            .id("region")
            .a11y_tag(quill::agent::ui_driver::redact::REGION_CHAT)
            .size_full()
            .bg(rgb(0xff_ffff))
            .child(row("chat=5", 0xff_0000))
            .child(row("chat=6", 0x00_ff00))
    }
}

/// A row the filter refuses under a container it draws is covered with
/// the filter's fill over its bounds, and none of its own paint is drawn.
#[gpui_kit::test]
fn a_refused_row_under_a_drawn_container_is_covered(cx: &mut TestAppContext) {
    use gpui_kit::{PaintFilter, SharedString, px, rgb, size};
    use quill::agent::ui_driver::redact;
    use quill::agent::ui_driver::surface::DriverScope;
    let handle = cx.open_window(size(px(200.), px(100.)), |_, _| Rows);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let filter = PaintFilter {
        allows: std::rc::Rc::new(|tags: &[SharedString]| {
            let innermost_first: Vec<String> = tags.iter().rev().map(|t| t.to_string()).collect();
            redact::point_allowed(&innermost_first, DriverScope::Everything, &|chat| chat != 5)
        }),
        hidden: crate::ui::agent_driver_shot::hidden_fill(),
        shown: rgb(0xff_ffff).into(),
    };
    let quads = vcx.update(|window, cx| {
        window.set_paint_filter(Some(filter));
        window.draw(cx).clear(cx);
        let quads = window.painted_quads();
        window.set_paint_filter(None);
        quads
    });
    let is = |q: &Quad, hex: u32| {
        let want = Rgba::from(rgb(hex));
        q.background.as_solid().map(Rgba::from).is_some_and(|c| {
            (c.r - want.r).abs() < 0.01
                && (c.g - want.g).abs() < 0.01
                && (c.b - want.b).abs() < 0.01
        })
    };
    assert!(
        quads.iter().any(|q| is(q, 0xff_ffff)),
        "the region is drawn"
    );
    assert!(
        quads.iter().any(|q| is(q, 0x00_ff00)),
        "the granted row is drawn"
    );
    assert!(
        !quads.iter().any(|q| is(q, 0xff_0000)),
        "nothing of the refused row"
    );
    assert_eq!(
        quads.iter().filter(|q| is(q, 0x00_00ff)).count(),
        1,
        "only the granted row's child"
    );
    let covers: Vec<_> = quads.iter().filter(|q| is_hidden_fill(q)).collect();
    let scale = vcx.update(|window, _| window.scale_factor());
    assert_eq!(covers.len(), 1, "{covers:?}");
    assert!(covers.iter().all(|q| {
        let b = quad_bounds(q);
        (b.size.width - 100.0 * scale).abs() < 1.0 && (b.size.height - 40.0 * scale).abs() < 1.0
    }));

    // The granted scope refuses paint outside every tag: the window starts
    // covered, and the allowed region gets the `shown` fill under it.
    let filter = PaintFilter {
        allows: std::rc::Rc::new(|tags: &[SharedString]| {
            let innermost_first: Vec<String> = tags.iter().rev().map(|t| t.to_string()).collect();
            redact::point_allowed(&innermost_first, DriverScope::Granted, &|chat| chat != 5)
        }),
        hidden: crate::ui::agent_driver_shot::hidden_fill(),
        shown: rgb(0x12_3456).into(),
    };
    let quads = vcx.update(|window, cx| {
        window.set_paint_filter(Some(filter));
        window.draw(cx).clear(cx);
        let quads = window.painted_quads();
        window.set_paint_filter(None);
        quads
    });
    let full = |q: &&Quad| (quad_bounds(q).size.width - 200.0 * scale).abs() < 1.0;
    let order: Vec<&str> = quads
        .iter()
        .filter_map(|q| {
            if is_hidden_fill(q) {
                Some(if full(&q) {
                    "cover-window"
                } else {
                    "cover-row"
                })
            } else if is(q, 0x12_3456) && full(&q) {
                Some("shown")
            } else if is(q, 0xff_ffff) {
                Some("region")
            } else if is(q, 0x00_ff00) {
                Some("granted-row")
            } else if is(q, 0xff_0000) {
                Some("refused-row")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        order,
        [
            "cover-window",
            "shown",
            "region",
            "cover-row",
            "granted-row"
        ],
        "painted in this order"
    );

    // Without a filter, all of it is drawn again.
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let quads = vcx.update(|window, _| window.painted_quads());
    assert!(quads.iter().any(|q| is(q, 0xff_0000)));
    assert!(!quads.iter().any(is_hidden_fill));
}

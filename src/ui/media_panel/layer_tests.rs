//! The panel's custom emoji play on its animation layer: a new frame redraws
//! the layer, and neither the composer nor `QuillApp`. Over the demo, with a
//! looping sticker behind every custom emoji.

use super::status_demo::STATUS_FILE;
use super::{PanelCell, PanelRow, PanelTab};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::sticker_playback::player::DecodedFrames;
use quill::telegram::envelope::StickerFormat;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-emoji-status-account");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        view.read(cx).playback.stickers.set_fixture(
            STATUS_FILE,
            Arc::new(|edge| DecodedFrames {
                frames: (0..30u8)
                    .map(|i| vec![i; (edge as usize).pow(2) * 4])
                    .collect(),
                fps: 30.0,
            }),
        );
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, _| window.activate_window());
    let _ = frames_over(&mut vcx, 5);
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.stickers.emoji.custom_emoji_stickers.update(|list| {
            for item in list {
                item.format = StickerFormat::Tgs;
            }
        });
        session.revision += 1;
    });
    (app, vcx)
}

fn custom_cells(app: &QuillApp) -> usize {
    let rows = app.pickers.media_panel.rows.iter();
    let cells = rows.filter_map(|row| match row {
        PanelRow::Cells { cells, .. } => Some(cells),
        PanelRow::Header { .. } => None,
    });
    cells
        .flatten()
        .filter(|cell| matches!(cell, PanelCell::Custom { .. }))
        .count()
}

/// Lets the clips open, then what two seconds of playing draw.
fn playing(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> std::collections::BTreeMap<&'static str, u32> {
    for _ in 0..4 {
        // What the sticker pump does in the app.
        app.update(vcx, |app, cx| {
            app.playback.stickers.settle();
            cx.notify();
        });
        let _ = frames_over(vcx, 1);
    }
    frames_over(vcx, 2)
}

/// The panel's cells are on its layer, playing there.
fn assert_panel_plays(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let wanted = app.read_with(vcx, |app, _| app.panel_layer().map(|layer| layer.demand()));
    let (layer, inline) = wanted.expect("the panel has its layer");
    assert!(layer > 0, "its cells ask for frames");
    assert_eq!(inline, 0, "and nothing falls back to the composer");
}

/// The chat list, the history and the panel's layers.
const LAYERS: u32 = 3;

/// `app` is what `QuillApp` may render meanwhile.
fn assert_only_the_layer_draws(
    drawn: &std::collections::BTreeMap<&'static str, u32>,
    app: std::ops::RangeInclusive<u32>,
) {
    let count = |kind: &str| drawn.get(kind).copied().unwrap_or(0);
    let frames = count("(window frame)");
    assert!(frames > 10, "they play: {drawn:?}");
    assert_eq!(count("animation layer"), frames * LAYERS, "{drawn:?}");
    assert!(app.contains(&count("app")), "{drawn:?}");
    for kind in ["sidebar", "conversation", "composer"] {
        assert_eq!(count(kind), 0, "{kind} stayed: {drawn:?}");
    }
}

/// Recently used custom emoji, which the emoji tab starts with.
fn use_custom_emoji(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        let ids = (5001..=5008).collect();
        session.settings.media_prefs.recent_custom_emoji_ids = ids;
        session.revision += 1;
    });
}

#[gpui_kit::test]
fn the_selector_plays_on_its_layer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_emoji_status_picker(window, cx)
    });
    let drawn = playing(&app, vcx);
    assert!(app.read_with(vcx, |app, _| custom_cells(app)) > 7);
    // Its search field's caret blinks.
    assert_only_the_layer_draws(&drawn, 0..=5);
    assert_panel_plays(&app, vcx);
}

#[gpui_kit::test]
fn the_panel_over_the_composer_plays_on_its_layer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    use_custom_emoji(&app, vcx);
    app.update(vcx, |app, cx| app.toggle_media_panel(PanelTab::Emoji, cx));
    let drawn = playing(&app, vcx);
    assert!(app.read_with(vcx, |app, _| custom_cells(app)) > 0);
    assert_only_the_layer_draws(&drawn, 0..=0);
    assert_panel_plays(&app, vcx);
}

/// A closed panel takes its layer with it.
#[gpui_kit::test]
fn closing_the_panel_stops_its_layer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    use_custom_emoji(&app, vcx);
    app.update(vcx, |app, cx| app.toggle_media_panel(PanelTab::Emoji, cx));
    assert_only_the_layer_draws(&playing(&app, vcx), 0..=0);
    assert_panel_plays(&app, vcx);
    app.update(vcx, |app, cx| {
        app.close_media_panel(cx);
    });
    let _ = frames_over(vcx, 1);
    let drawn = frames_over(vcx, 2);
    let count = |kind: &str| drawn.get(kind).copied().unwrap_or(0);
    let frames = count("(window frame)");
    assert_eq!(count("animation layer"), frames * (LAYERS - 1), "{drawn:?}");
}

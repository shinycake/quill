//! Floating menus wear Web K's shape: `.btn-menu` in tweb's
//! `scss/partials/_button.scss` is `$border-radius-medium` (16px) with
//! `0 0 10px rgba(0, 0, 0, .15)` in both themes, and its rows clip to the
//! corners. Classic menus keep the kit's.
//!
//! In dark the cards are near black, the menu's own fill is too, and a 15%
//! black shadow does not show on them, so a menu would merge with the card
//! under it. Web K's dark menu is lighter than its columns; here the menu
//! takes a fill a step lighter than the cards, a faint light edge and a
//! deeper shadow.
//!
//! Every window notes its look (`QuillApp::note_window_width`) before it
//! builds anything else, and a menu reads the look of the window it is in:
//! its shape here, its entrance in `menu_pop`.

use gpui_kit::component::popover_shape::{MenuEnter, PopoverShape, PopoverShapeHook};
use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use quill::layout_style::Look;
use std::collections::HashMap;

/// Web K's menu corner radius.
pub(in crate::ui) const MENU_RADIUS: f32 = 16.;

/// Each open window's look, as its last frame noted it.
#[derive(Default)]
struct WindowLooks(HashMap<WindowId, Look>);

impl Global for WindowLooks {}

/// The look of `window`: Classic's until it has drawn.
fn look_of(window: &Window, cx: &App) -> Look {
    cx.try_global::<WindowLooks>()
        .and_then(|looks| looks.0.get(&window.window_handle().window_id()).copied())
        .unwrap_or(Look::Lines)
}

/// Web K's menu shadow; deeper in dark, where its 15% black would not show.
pub(in crate::ui) fn menu_shadow(dark: bool) -> Vec<BoxShadow> {
    if dark {
        return vec![BoxShadow {
            color: hsla(0., 0., 0., 0.55),
            offset: point(px(0.), px(4.)),
            blur_radius: px(18.),
            spread_radius: px(0.),
            inset: false,
        }];
    }
    vec![BoxShadow {
        color: hsla(0., 0., 0., 0.15),
        offset: point(px(0.), px(0.)),
        blur_radius: px(10.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// How much lighter a dark menu is than the theme's popover fill.
const DARK_MENU_LIFT: f32 = 0.10;

/// The dark menu's fill and edge (`None, None` in light).
fn dark_surface(cx: &App) -> (Option<Hsla>, Option<Hsla>) {
    match cx.try_global::<Theme>() {
        Some(theme) if theme.is_dark() => {
            let base = theme.popover;
            let fill = hsla(base.h, base.s, (base.l + DARK_MENU_LIFT).min(1.), 1.);
            (Some(fill), Some(hsla(0., 0., 1., 0.10)))
        }
        _ => (None, None),
    }
}

/// The shape menus in `window` take: `Some` in Floating.
pub(in crate::ui) fn menu_shape(window: &Window, cx: &App) -> Option<PopoverShape> {
    menu_shape_in(look_of(window, cx), cx)
}

/// The shape menus take in a window with this look.
pub(in crate::ui) fn menu_shape_in(look: Look, cx: &App) -> Option<PopoverShape> {
    match look {
        Look::Lines => None,
        Look::Cards => {
            let (fill, edge) = dark_surface(cx);
            Some(PopoverShape {
                radius: px(MENU_RADIUS),
                shadow: menu_shadow(edge.is_some()),
                fill,
                edge,
                enter: MenuEnter::Pop,
                dialog_radius: dialog_radius(look, cx),
                section_radius: px(SECTION_RADIUS),
            })
        }
    }
}

/// A settings group's corners: Web K's `$border-radius-medium` (16).
/// Its sections are rounder (`$section-rounded-border-radius`, 24) but
/// run edge to edge; Quill's group holds rows inset by 4 pt whose hover
/// fill has its own small radius, and 16 keeps the two concentric.
const SECTION_RADIUS: f32 = 16.;

/// A dialog's corners in a window with this look: the card radius, so a
/// box over the wallpaper is rounded as the cards around it (Web K's
/// popups are rounder still, `--popup-border-radius: 2.5rem`).
fn dialog_radius(look: Look, cx: &App) -> Pixels {
    match cx.try_global::<Theme>() {
        Some(theme) => super::radius::card_in(quill::layout_style::metrics_of(look), theme),
        None => px(MENU_RADIUS),
    }
}

/// How menus in a window with this look enter.
pub(in crate::ui) fn menu_entrance(look: Look) -> MenuEnter {
    match look {
        Look::Cards => MenuEnter::Pop,
        Look::Lines => MenuEnter::FadeSlide,
    }
}

/// `window` is about to draw with this look; the kit's popovers ask
/// through the same answer. A window seen for the first time drops the
/// looks of windows that have closed.
pub(in crate::ui) fn note_drawn_window(window: WindowId, look: Look, cx: &mut App) {
    if !cx.has_global::<PopoverShapeHook>() {
        cx.set_global(PopoverShapeHook(menu_shape));
    }
    let known = cx
        .try_global::<WindowLooks>()
        .and_then(|looks| looks.0.get(&window).copied());
    match known {
        Some(known) if known == look => {}
        Some(_) => {
            cx.default_global::<WindowLooks>().0.insert(window, look);
        }
        None => {
            let open: Vec<WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
            let looks = &mut cx.default_global::<WindowLooks>().0;
            looks.retain(|id, _| open.contains(id));
            looks.insert(window, look);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "menu_shape_tests.rs"]
mod tests;

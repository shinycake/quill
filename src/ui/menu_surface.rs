//! The look of Quill's own pop-up menus (message, chat, archive, folder,
//! story and link menus): the kit popover surface, a hairline border and
//! a soft shadow. They used to wear an accent-coloured border, which read
//! as a stuck focus ring.
#![deny(clippy::indexing_slicing)]

use crate::ui::chrome::menu_shape::menu_shape_in;
use crate::ui::glass::{GlassRole, GlassSurface as _};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::popover_shape::concentric_radius;
use gpui_kit::*;
use quill::layout_style::Look;

pub(crate) trait MenuSurface: Styled + Sized {
    /// Rounded popover surface with the theme's border and a shadow, or
    /// glass while glass effects are on (`ui/glass`). The glass keeps a
    /// clear hairline where the border was, so rows sit where they do on
    /// the solid surface.
    ///
    /// In Floating it takes Web K's shape instead (`chrome::menu_shape`):
    /// the corner radius and shadow, rows clipped to the corners.
    fn menu_surface(self, look: Look, cx: &App) -> Self {
        let theme = cx.theme();
        let shape = menu_shape_in(look, cx);
        let this = match &shape {
            Some(shape) => self.rounded(shape.radius).clip_rounded(),
            None => self.rounded(menu_radius(look, cx)),
        };
        this.border_1()
            .text_color(theme.popover_foreground)
            .glass_or(GlassRole::Menu, cx, |this| match shape {
                Some(shape) => this
                    .border_color(shape.edge.unwrap_or(theme.border))
                    .bg(shape.fill.unwrap_or(theme.popover))
                    .shadow(shape.shadow),
                None => this
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .shadow_lg(),
            })
    }
}

impl<E: Styled> MenuSurface for E {}

/// The corner radius of a menu in a window with this look: Web K's in
/// Floating, the theme's in Classic.
pub(crate) fn menu_radius(look: Look, cx: &App) -> Pixels {
    menu_shape_in(look, cx).map_or(cx.theme().radius, |shape| shape.radius)
}

/// The corner radius of a row in a menu: concentric with the menu's
/// corners, `PANEL_INSET` inside them.
pub(crate) fn menu_row_radius(look: Look, cx: &App) -> Pixels {
    concentric_radius(menu_radius(look, cx), px(PANEL_INSET))
}

pub(super) trait MenuRowCorners: Styled + Sized {
    /// A menu row's corners, concentric with the menu drawing it (the
    /// window notes its look before any row draws).
    fn menu_row_corners(
        self,
        items: &crate::ui::context_menu_keys::ContextMenuItems,
        cx: &App,
    ) -> Self {
        match items.look() {
            Some(look) => self.rounded(menu_row_radius(look, cx)),
            None => self,
        }
    }
}

impl<E: Styled> MenuRowCorners for E {}

pub(crate) trait RowHighlight: InteractiveElement + Styled + Sized {
    /// The row is lit (keyboard or selection): a glass highlight while
    /// glass effects are on, `fill` otherwise.
    fn lit(self, fill: Hsla, cx: &App) -> Self {
        match crate::ui::glass::lens(GlassRole::Highlight, cx) {
            Some(lens) => self.backdrop_glass(Some(lens)),
            None => self.bg(fill),
        }
    }

    /// The pointer lights the row: a glass highlight while glass effects
    /// are on, `fill` otherwise.
    fn hover_highlight(self, fill: Hsla, cx: &App) -> Self {
        match crate::ui::glass::lens(GlassRole::Highlight, cx) {
            Some(lens) => self.hover(move |style| style.backdrop_glass(Some(lens))),
            None => self.hover(move |style| style.bg(fill)),
        }
    }
}

impl<E: InteractiveElement + Styled> RowHighlight for E {}

/// Width of a context menu panel and of the submenu beside it.
pub(crate) const MENU_WIDTH: f32 = 240.;
/// A menu panel's inner padding, which a submenu's top rows level with.
pub(crate) const PANEL_PADDING: f32 = 4.;
/// From a menu panel's outer edge to its rows: the hairline border and the
/// padding.
const PANEL_INSET: f32 = PANEL_PADDING + 1.;

/// A submenu beside its parent row (a child of that row), its top rows
/// level with it. As tdesktop's `PopupMenu::prepareGeometryFor` does, it
/// opens to the right of the parent menu and moves to its left when the
/// right has no room; near the top or bottom edge it moves back inside the
/// window.
///
/// `on_bounds` gets the panel's bounds as it prepaints and `on_pointer` each
/// pointer move over it (the parent menu's aim, `menu_aim`).
pub(crate) fn submenu_panel(
    id: &'static str,
    rows: Vec<AnyElement>,
    on_bounds: impl FnOnce(Bounds<Pixels>) + 'static,
    on_pointer: impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static,
    look: Look,
    cx: &App,
) -> AnyElement {
    // The parent menu's outer left and right edges, relative to the row,
    // which sits `PANEL_INSET` inside them.
    let parent_menu = Bounds::new(
        point(px(-PANEL_INSET), px(0.)),
        size(px(MENU_WIDTH), px(0.)),
    );
    let panel = anchored()
        .position_mode(AnchoredPositionMode::Local)
        .attach(parent_menu, Anchor::TopRight)
        .anchor(Anchor::TopLeft)
        .offset(point(px(0.), px(-PANEL_INSET)))
        .snap_to_window_with_margin(px(8.))
        .child(
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .occlude()
                .flex()
                .flex_col()
                .w(px(MENU_WIDTH))
                .p_1()
                .menu_surface(look, cx)
                .on_mouse_move(on_pointer)
                .children(rows)
                .on_prepaint(move |bounds, _, _| on_bounds(bounds)),
        );
    // Measured from the row's own corner, not from where its padding puts
    // the next child.
    div()
        .absolute()
        .top_0()
        .left_0()
        .child(panel)
        .into_any_element()
}

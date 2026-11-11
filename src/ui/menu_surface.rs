//! The look of Quill's own pop-up menus (message, chat, archive, folder,
//! story and link menus): the kit popover surface, a hairline border and
//! a soft shadow. They used to wear an accent-coloured border, which read
//! as a stuck focus ring.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

pub(crate) trait MenuSurface: Styled + Sized {
    /// Rounded popover surface with the theme's border and a shadow.
    fn menu_surface(self, cx: &App) -> Self {
        let theme = cx.theme();
        self.rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
    }
}

impl<E: Styled> MenuSurface for E {}

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
pub(crate) fn submenu_panel(id: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
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
                .menu_surface(cx)
                .children(rows),
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

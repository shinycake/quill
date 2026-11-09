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

/// A submenu beside its parent row (a child of that row), its top rows
/// level with it, moved back inside the window when the row sits near an
/// edge.
pub(crate) fn submenu_panel(id: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
    anchored()
        .offset(point(px(MENU_WIDTH - PANEL_PADDING), px(-PANEL_PADDING)))
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
        )
        .into_any_element()
}

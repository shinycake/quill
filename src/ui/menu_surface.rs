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

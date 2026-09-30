//! PressableDiv helper.

use gpui_kit::component::*;
use gpui_kit::*;

/// kit Phase 9: hover/pressed feedback for hand-rolled clickable surfaces.
/// Kit buttons carry state styles by default; this gives the remaining
/// custom `cursor_pointer` divs the same affordance — a theme-driven hover
/// tint plus a slightly stronger pressed state.
pub(super) trait PressableDiv {
    fn pressable(self, theme: &Theme) -> Self;
}

impl PressableDiv for gpui_kit::Stateful<Div> {
    fn pressable(self, theme: &Theme) -> Self {
        self.hover(|s| s.bg(theme.accent.opacity(0.10)))
            .active(|s| s.bg(theme.accent.opacity(0.22)))
    }
}

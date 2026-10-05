//! PressableDiv helper.

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
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

/// Full-width, left-aligned action row for panels and sheets (kit buttons
/// always center their label). Focusable and keyboard-activatable; the
/// caller attaches `on_click`. `destructive` paints label and icon in the
/// danger color.
pub(super) fn action_row(
    id: impl Into<ElementId>,
    icon: Option<gpui_kit::assets::IconName>,
    label: impl Into<SharedString>,
    destructive: bool,
    cx: &App,
) -> gpui_kit::Stateful<Div> {
    let label = label.into();
    let color = if destructive {
        cx.theme().danger
    } else {
        cx.theme().foreground
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(color)
        .role(Role::Button)
        .aria_label(label.clone())
        .tab_index(0)
        .hover(|s| s.bg(cx.theme().secondary))
        .active(|s| s.bg(cx.theme().secondary_active))
        .when_some(icon, |this, icon| {
            this.child(Icon::new(icon).size(px(16.)).text_color(color))
        })
        .child(label)
}

//! PressableDiv helper: the states a hand-rolled clickable surface shows.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The keyboard focus ring's width (the kit's buttons draw a 2 px ring).
const RING_WIDTH: f32 = 2.;

/// kit Phase 9: hover/pressed feedback for hand-rolled clickable surfaces.
/// Kit buttons carry state styles by default; this gives the remaining
/// custom `cursor_pointer` divs the same affordance — a theme-driven hover
/// tint plus a slightly stronger pressed state — and, once the element is
/// a tab stop (`tab_index`), the shared focus ring while the keyboard
/// focuses it.
pub(super) trait PressableDiv {
    fn pressable(self, theme: &Theme) -> Self;
    /// The focus ring alone, for a surface with hover feedback of its own.
    fn focus_ring_visible(self, theme: &Theme) -> Self;
}

impl PressableDiv for gpui_kit::Stateful<Div> {
    /// tdesktop's neutral over/pressed fills (`windowBgOver` /
    /// `windowBgRipple`): a light gray over the background in the day
    /// theme and a light lift in the night theme, not an accent tint.
    fn pressable(self, theme: &Theme) -> Self {
        let fg = theme.foreground;
        let over = hover_fill(theme);
        self.hover(move |s| s.bg(over))
            .active(move |s| s.bg(fg.opacity(0.11)))
            .focus_ring_visible(theme)
    }

    fn focus_ring_visible(self, theme: &Theme) -> Self {
        let ring = focus_ring(theme);
        self.focus_visible(move |s| s.shadow(vec![ring]))
    }
}

/// The keyboard focus ring: inside the element's edge in the theme's
/// focus colour, so it takes no room and reads apart from hover (a fill).
pub(super) fn focus_ring(theme: &Theme) -> BoxShadow {
    BoxShadow {
        color: theme.ring,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(RING_WIDTH),
        inset: true,
    }
}

/// The hover fill of a pressable surface; the chat list also paints its
/// keyboard selection with it (tdesktop's selected row is the hovered one).
pub(super) fn hover_fill(theme: &Theme) -> Hsla {
    theme.foreground.opacity(0.06)
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
        .focus_ring_visible(cx.theme())
        .when_some(icon, |this, icon| {
            this.child(Icon::new(icon).size(px(16.)).text_color(color))
        })
        .child(label)
}

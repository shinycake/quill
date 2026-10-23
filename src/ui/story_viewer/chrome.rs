//! The story viewer's colors. tdesktop draws the viewer on the dark media
//! viewer background in every theme, and its controls take the group call
//! palette (`media_view.style`: `storiesComposeBg` = `groupCallMembersBg`,
//! `storiesComposeWhiteText`, `storiesComposeGrayText`,
//! `storiesComposeGrayIcon`, `storiesComposeBlue`). So the chrome here is
//! light on dark whatever the app theme is; boxes opened from the viewer
//! keep the app theme.
#![deny(clippy::indexing_slicing)]

use crate::ui::group_call_panel::{
    ACTIVE_FG, BG, MEMBER_INACTIVE_ICON, MEMBER_NOT_JOINED, MEMBERS_BG, MEMBERS_BG_OVER,
};
use gpui_kit::component::button::ButtonCustomVariant;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::{
    AnyElement, App, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, div, hsla, px, rgb,
};

/// Names, labels and icons over the viewer's backdrop.
pub(in crate::ui) fn fg() -> Hsla {
    hsla(0., 0., 1., 1.)
}

/// Secondary text: counts, dates, notes (white at 60%).
pub(in crate::ui) fn secondary() -> Hsla {
    hsla(0., 0., 1., 0.6)
}

/// `storiesComposeBg`: the comment field and its buttons.
pub(in crate::ui) fn compose_bg() -> Hsla {
    rgb(MEMBERS_BG).into()
}

/// `storiesComposeBgOver`.
pub(in crate::ui) fn compose_bg_over() -> Hsla {
    rgb(MEMBERS_BG_OVER).into()
}

/// `storiesComposeGrayText`: the field's placeholder.
pub(in crate::ui) fn gray_text() -> Hsla {
    rgb(MEMBER_NOT_JOINED).into()
}

/// `storiesComposeGrayIcon`: the composer's icons.
pub(in crate::ui) fn gray_icon() -> Hsla {
    rgb(MEMBER_INACTIVE_ICON).into()
}

/// `storiesComposeBlue`: the send icon, links in the toast.
pub(in crate::ui) fn blue() -> Hsla {
    rgb(ACTIVE_FG).into()
}

/// `groupCallBg`: behind a picture, an admin's comment.
pub(in crate::ui) fn frame_bg() -> Hsla {
    rgb(BG).into()
}

/// A chosen heart reaction.
pub(in crate::ui) fn heart() -> Hsla {
    rgb(0xff5a6e).into()
}

/// The gold of the Stars icon on the Stars button and in the toast.
pub(in crate::ui) fn stars_gold() -> Hsla {
    rgb(0xffb800).into()
}

/// A text button over the backdrop: white, a light wash on hover (the
/// kit's ghost button takes the app theme's hover).
pub(in crate::ui) fn ghost(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(hsla(0., 0., 0., 0.))
        .foreground(fg())
        .hover(hsla(0., 0., 1., 0.1))
        .active(hsla(0., 0., 1., 0.16))
        .shadow(false)
}

/// A round icon button over the story's picture: a dark wash so it reads on
/// any photo, white icon.
pub(in crate::ui) fn over_media(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(hsla(0., 0., 0., 0.4))
        .foreground(fg())
        .hover(hsla(0., 0., 0., 0.6))
        .active(hsla(0., 0., 0., 0.7))
        .shadow(false)
}

/// A composer button on `storiesComposeBg` with a gray icon.
pub(in crate::ui) fn compose(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(compose_bg())
        .foreground(gray_icon())
        .hover(compose_bg_over())
        .active(compose_bg_over())
        .shadow(false)
}

/// A search field on the viewer's chrome: `storiesComposeBg`, white text
/// and the placeholder in `storiesComposeGrayText` (the kit field's own
/// look and placeholder follow the app theme, so both are off).
pub(in crate::ui) fn search_field(
    input: &Entity<TextareaState>,
    placeholder: SharedString,
    selector: &'static str,
    cx: &App,
) -> AnyElement {
    let empty = input.read(cx).value().is_empty();
    let mut field = div()
        .relative()
        .h(px(32.))
        .pl(px(10.))
        .rounded(px(8.))
        .bg(compose_bg())
        .child(
            Textarea::new(input)
                .appearance(false)
                .bordered(false)
                .text_color(fg())
                .aria_label(placeholder.clone())
                .h(px(32.)),
        );
    if empty {
        field = field.child(
            div()
                .debug_selector(move || selector.into())
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(12.))
                .right_0()
                .flex()
                .items_center()
                .truncate()
                .text_color(gray_text())
                .child(placeholder),
        );
    }
    field.into_any_element()
}

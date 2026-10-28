//! Scroll-triggered paging, the way Telegram Desktop loads long lists: the
//! next page is asked for when the reader scrolls near the end (or when the
//! page loaded does not fill the view yet), and a spinner row shows while it
//! is in flight. There is no "Load more" button.
//!
//! The decision is pure ([`near_end`], [`load_due`]) so it can be tested; the
//! rest is thin glue: [`scrolled_near_end`] reads a `ScrollHandle`,
//! [`after_layout`] looks again once a frame is laid out, and
//! [`loading_row`] is the shared spinner row.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// How close to the end of a list (in logical pixels) the next page is asked
/// for.
pub(super) const NEAR_END_PX: f32 = 160.;

/// The list is scrolled to within `threshold` of its end. Offsets run
/// negative as the list scrolls down. A list that is not laid out yet
/// (`viewport` is zero) is never near its end: the next frame asks again.
/// A list whose content fits the view has `max_offset` zero, so it is near
/// its end at once, which is how a first page that does not fill the view
/// triggers the next one.
pub(super) fn near_end(
    viewport: Pixels,
    max_offset: Pixels,
    offset: Pixels,
    threshold: Pixels,
) -> bool {
    viewport > px(0.) && max_offset + offset <= threshold
}

/// Whether to ask for another page: the reader is near the end, the server
/// has more, and no page is in flight (so one scroll never sends two
/// requests).
pub(super) fn load_due(near_end: bool, has_more: bool, in_flight: bool) -> bool {
    near_end && has_more && !in_flight
}

/// `scroll` is within [`NEAR_END_PX`] of its end.
pub(super) fn scrolled_near_end(scroll: &ScrollHandle) -> bool {
    scrolled_within(scroll, px(NEAR_END_PX))
}

/// `scroll` is within `threshold` of its end.
pub(super) fn scrolled_within(scroll: &ScrollHandle, threshold: Pixels) -> bool {
    near_end(
        scroll.bounds().size.height,
        scroll.max_offset().y,
        scroll.offset().y,
        threshold,
    )
}

/// Run `check` once this frame is laid out. A page that arrived and left the
/// list short of the view, or a list that just opened, then asks for the next
/// page without waiting for a scroll.
pub(super) fn after_layout<T: 'static>(
    cx: &mut Context<T>,
    check: impl FnOnce(&mut T, &mut Context<T>) + 'static,
) {
    let entity = cx.entity();
    cx.defer(move |cx| {
        entity.update(cx, check);
    });
}

/// The list length a page was last asked for at, for lists whose end is
/// known only by a count that the server may never reach (hidden entries).
/// An automatic look after layout asks once per length, so an empty page
/// cannot start a loop; a scroll by the reader always may ask again.
#[derive(Default)]
pub(crate) struct Asked(std::cell::Cell<Option<usize>>);

impl Asked {
    /// Whether to ask at `loaded` items: always when `scrolled`, otherwise
    /// only the first time at this length.
    pub(super) fn allow(&self, loaded: usize, scrolled: bool) -> bool {
        let fresh = self.0.get() != Some(loaded);
        self.0.set(Some(loaded));
        scrolled || fresh
    }
}

/// Wrap `body` (a list inside a scroll region tracked by a handle) so that
/// `check` runs when the reader scrolls (`scrolled` is true) and once after
/// every frame (`scrolled` is false: a first page that does not fill the
/// view, or a page that just arrived). `check` decides with [`load_due`], must
/// not notify unless it sent a request, and should not retry a failed page
/// unless `scrolled`.
pub(super) fn watch<T: 'static>(
    body: impl IntoElement,
    cx: &mut Context<T>,
    check: fn(&mut T, &mut Context<T>, bool),
) -> Div {
    after_layout(cx, move |this, cx| check(this, cx, false));
    div()
        .on_scroll(cx.listener(move |this, _, _, cx| check(this, cx, true)))
        .child(body)
}

/// The wheel listener for a scroll container that is itself laid out by a
/// parent (so it cannot be wrapped in another element): `check` runs on
/// every wheel event over it and once after every frame, as in [`watch`].
pub(super) fn watch_scroll<T: 'static>(
    cx: &mut Context<T>,
    check: fn(&mut T, &mut Context<T>, bool),
) -> impl Fn(&ScrollEvent, &mut Window, &mut App) + 'static {
    after_layout(cx, move |this, cx| check(this, cx, false));
    cx.listener(move |this, _, _, cx| check(this, cx, true))
}

/// The row at the end of a list while the next page loads: a spinner and
/// the "Loading…" text of Telegram Desktop.
pub(super) fn loading_row(id: impl Into<ElementId>, cx: &App) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap_2()
        .py_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .role(Role::Status)
        .aria_label(tr(keys::PROFILE_LOADING))
        .child(Spinner::new().small())
        .child(tr(keys::PROFILE_LOADING))
}

#[cfg(test)]
mod tests {
    use super::{Asked, load_due, near_end};
    use gpui_kit::px;

    #[test]
    fn near_the_end_only() {
        let t = px(160.);
        assert!(!near_end(px(420.), px(1000.), px(0.), t));
        assert!(!near_end(px(420.), px(1000.), px(-800.), t));
        assert!(near_end(px(420.), px(1000.), px(-850.), t));
        assert!(near_end(px(420.), px(1000.), px(-1000.), t));
    }

    #[test]
    fn a_short_first_page_is_near_the_end_at_once() {
        assert!(near_end(px(420.), px(0.), px(0.), px(160.)));
    }

    #[test]
    fn a_list_not_laid_out_yet_asks_for_nothing() {
        assert!(!near_end(px(0.), px(0.), px(0.), px(160.)));
    }

    #[test]
    fn an_automatic_look_asks_once_per_length() {
        let asked = Asked::default();
        assert!(asked.allow(10, false));
        assert!(!asked.allow(10, false));
        assert!(asked.allow(10, true));
        assert!(asked.allow(20, false));
    }

    #[test]
    fn one_request_at_a_time() {
        assert!(load_due(true, true, false));
        assert!(!load_due(true, true, true));
        assert!(!load_due(true, false, false));
        assert!(!load_due(false, true, false));
    }
}

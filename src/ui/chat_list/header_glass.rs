//! The chat list under a glass header: with Glass effects on, the header,
//! folder tabs and search field float over the rows as one frosted bar
//! (`GlassRole::Bar`) and the rows scroll under it.
//!
//! The rows keep their place: the list's top padding is the bar's height,
//! measured at paint ([`QuillApp::chat_list_top_inset`]), so the first
//! row starts where it started under the solid header, rows scrolled up
//! still draw under the bar, and the kit list keeps keyboard selection out
//! from under it (`scroll_to_item` clamps to the padded content). With the
//! switch off this module draws nothing and the column is built as before.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::glass::GlassRole;
use std::cell::Cell;

/// The floating bar's height at the last paint. Zero until the bar first
/// paints, so the first frame lays the rows from the top and the next
/// one (one frame later) puts them under the bar.
#[derive(Clone, Default)]
pub(in crate::ui) struct HeaderInset(Rc<Cell<Pixels>>);

impl HeaderInset {
    pub(in crate::ui) fn get(&self) -> Pixels {
        self.0.get()
    }
}

/// How far the bar's glass reaches past the column's top and sides: past
/// the rim of the Bar recipe, so the column's clip hides it.
const BAR_REACH: f32 = 24.0;

impl QuillApp {
    /// The bar's look while the chat rows show and the look is glass.
    pub(super) fn chat_list_bar_glass(&self, cx: &App) -> Option<LensStyle> {
        (self.chat_list.tab == SidebarTab::Chats && !self.search_is_open())
            .then(|| crate::ui::glass::lens(GlassRole::Bar, cx))
            .flatten()
    }

    /// The list's top padding: the bar's height while it floats.
    pub(super) fn chat_list_top_inset(&self, cx: &App) -> Pixels {
        if self.chat_list_bar_glass(cx).is_some() {
            self.chat_list.header_inset.get()
        } else {
            px(0.)
        }
    }

    /// The column `header` (built with every header part as before) turned
    /// into a glass bar over `rows`. The column keeps its id, focus and
    /// key handlers, so the keyboard reaches the list as before.
    pub(super) fn float_chat_list_header(
        &self,
        header: Stateful<Div>,
        rows: AnyElement,
        lens: LensStyle,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let m = self.density();
        let inset = self.chat_list.header_inset.clone();
        let app = cx.weak_entity();
        let screen = self.screen_epoch();
        // Measures the bar and renders again when its height changed (a
        // folder strip or the select bar came or went).
        let probe = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let height = bounds.size.height;
                if inset.0.replace(height) != height {
                    let app = app.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = app.update(cx, |this, cx| {
                            this.in_screen_scope(screen, cx, |_, cx| cx.notify());
                        });
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        // The glass reaches past the column's top and sides, where the
        // column clips it: only its lower edge shows, the line the rows
        // pass under.
        let reach = px(BAR_REACH);
        let backing = div()
            .absolute()
            .top(-reach)
            .left(-reach)
            .right(-reach)
            .bottom_0()
            .backdrop_glass(Some(lens));
        let bar = div()
            .debug_selector(|| "chat-list-glass-bar".into())
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .child(backing)
            .child(
                header
                    .relative()
                    .w_full()
                    .h_auto()
                    .pb(px(m.sidebar_gap))
                    .border_r_0()
                    .bg(gpui_kit::transparent_black()),
            )
            .child(probe);
        div()
            .id("sidebar-glass")
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .w(self.list_width())
            .flex_none()
            .h_full()
            .px(px(m.sidebar_inset_x))
            .pb(px(m.sidebar_inset_y))
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div().relative().flex_1().min_h_0().w_full().child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(rows),
                ),
            )
            .child(bar)
    }
}

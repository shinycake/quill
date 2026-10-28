//! The strip of tabs under the global search field (tdesktop
//! `Suggestions::setupTabs`): Chats, Channels, Apps, Posts, the media tabs
//! and, while the download list has entries, Downloads. One row that
//! scrolls sideways (the vertical wheel too, as tdesktop's
//! `setCustomWheelProcess` does). Picking a tab moves the search filters
//! onto it ([`SearchTab::apply`]) and the driver reruns the query.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn search_tab_strip(
        &self,
        active: SearchTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = self.search_ui.tabs_scroll.clone();
        let chips: Vec<AnyElement> = SearchTab::strip(self.has_downloads())
            .into_iter()
            .map(|tab| {
                div()
                    .flex_none()
                    .child(Self::filter_chip(
                        ("search-tab", tab as u64),
                        tab.label(),
                        tab == active,
                        move |this, _, cx| this.pick_search_tab(tab, cx),
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        let wheel_handle = handle.clone();
        // Edge fades say "more this way": a clipped strip with no cue reads
        // as a bug (Design Guides, scroll regions need a deliberate edge).
        let offset = handle.offset().x;
        let max = handle.max_offset().x;
        let fade_left = offset < px(-1.);
        // Before the first layout the strip's size is unknown: assume it
        // overflows (six or more tabs) and re-render once measured.
        let measured = handle.bounds().size.width > px(0.);
        let fade_right = if measured {
            max > px(1.) && offset > -max + px(1.)
        } else {
            let entity = cx.entity();
            cx.defer(move |cx| entity.update(cx, |_, cx| cx.notify()));
            chips.len() >= 6
        };
        let bg = cx.theme().background;
        let fade = |id: &'static str, toward_right: bool| {
            let (from, to) = if toward_right {
                (bg.opacity(0.), bg)
            } else {
                (bg, bg.opacity(0.))
            };
            div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(40.))
                .map(|this| {
                    if toward_right {
                        this.right_0()
                    } else {
                        this.left_0()
                    }
                })
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(from, 0.),
                    linear_color_stop(to, 1.),
                ))
        };
        let strip = div()
            .id("search-tabs")
            .w_full()
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll()
            .track_scroll(&handle)
            .on_scroll(move |event, window, cx| {
                // A vertical wheel moves the strip sideways.
                let delta = event.delta.pixel_delta(window.line_height());
                if delta.y.abs() > delta.x.abs() {
                    let offset = wheel_handle.offset();
                    let max = wheel_handle.max_offset().x;
                    let x = (offset.x + delta.y).clamp(-max, px(0.));
                    wheel_handle.set_offset(point(x, offset.y));
                    window.refresh();
                    cx.stop_propagation();
                }
            })
            .children(chips);
        div()
            .id("search-tabs-frame")
            .relative()
            .w_full()
            .child(strip)
            .when(fade_left, |this| {
                this.child(fade("search-tabs-fade-left", false))
            })
            .when(fade_right, |this| {
                this.child(fade("search-tabs-fade-right", true))
            })
            .into_any_element()
    }

    /// Move the global search onto `tab`, keeping the query, the date
    /// window and the archive toggle, and bring its chip into view.
    pub(in crate::ui) fn pick_search_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        let current = self
            .session()
            .map(|s| s.search.search.filters)
            .unwrap_or_default();
        if SearchTab::of(&current) == tab {
            return;
        }
        if let Some(index) = SearchTab::strip(self.has_downloads())
            .iter()
            .position(|shown| *shown == tab)
        {
            self.search_ui.tabs_scroll.scroll_to_item(index);
        }
        self.set_search_filters(tab.apply(current), cx);
    }
}

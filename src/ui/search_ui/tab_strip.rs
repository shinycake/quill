//! The strip of tabs under the global search field (tdesktop
//! `Suggestions::setupTabs`): Chats, Channels, Apps, Posts and the media
//! tabs. Picking one moves the search filters onto it
//! ([`SearchTab::apply`]) and the driver reruns the query.

use super::*;

impl QuillApp {
    /// The strip as wrapping chips: all ten tabs stay in view in the narrow
    /// sidebar, with the active one marked (a scrolling bar hid the active
    /// tab behind its overflow menu).
    pub(in crate::ui) fn search_tab_strip(
        &self,
        active: SearchTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips: Vec<AnyElement> = SearchTab::ALL
            .into_iter()
            .map(|tab| {
                Self::filter_chip(
                    ("search-tab", tab as u64),
                    tab.label(),
                    tab == active,
                    move |this, _, cx| this.pick_search_tab(tab, cx),
                    cx,
                )
                .into_any_element()
            })
            .collect();
        div()
            .id("search-tabs")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .children(chips)
            .into_any_element()
    }

    /// Move the global search onto `tab`, keeping the query, the date
    /// window and the archive toggle.
    pub(in crate::ui) fn pick_search_tab(&mut self, tab: SearchTab, cx: &mut Context<Self>) {
        let current = self
            .session()
            .map(|s| s.search.search.filters)
            .unwrap_or_default();
        if SearchTab::of(&current) == tab {
            return;
        }
        self.set_search_filters(tab.apply(current), cx);
    }
}

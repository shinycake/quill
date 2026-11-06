//! The narrowing of a typed search as two menu buttons in one row, the
//! way tdesktop puts its chat-type filter behind a popup on the results
//! bar: chat type and archive in one menu, the date window in the other.
//! They used to be three labelled rows of chips that pushed the results
//! half-way down the sidebar.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use quill::lang::{keys, tr};
use quill::search_filters::GlobalSearchFilters;

impl QuillApp {
    /// A menu button that reads quietly at its default and stands out once
    /// it narrows the search.
    fn filter_menu_button(id: &'static str, label: SharedString, narrowed: bool) -> Button {
        let button = Button::new(id).label(label).xsmall().dropdown_caret(true);
        if narrowed {
            button.secondary()
        } else {
            button.ghost()
        }
    }

    fn edit_search_filters(
        owner: &WeakEntity<QuillApp>,
        edit: impl Fn(&mut GlobalSearchFilters) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let owner = owner.clone();
        move |_, _, cx| {
            let _ = owner.update(cx, |this, cx| {
                let mut next = this
                    .session()
                    .map(|s| s.search.search.filters)
                    .unwrap_or_default();
                edit(&mut next);
                this.set_search_filters(next, cx);
            });
        }
    }

    /// "All chats ▾": the chat type (where the tab has one; Channels is a
    /// tab of its own) and From archive, tdesktop's menu.
    pub(super) fn search_type_menu(
        filters: GlobalSearchFilters,
        tab: SearchTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = if filters.archived && filters.chat_type == SearchChatType::All {
            tr(keys::SEARCH_FILTER_FROM_ARCHIVE)
        } else {
            filters.chat_type.label()
        };
        let narrowed = filters.archived || filters.chat_type != SearchChatType::All;
        let owner = cx.entity().downgrade();
        Self::filter_menu_button("search-filter-type", label.into(), narrowed).dropdown_menu(
            move |mut menu: PopupMenu, _, _| {
                menu = menu.item(
                    PopupMenuItem::new(tr(keys::SEARCH_FILTER_FROM_ARCHIVE))
                        .checked(filters.archived)
                        .on_click(Self::edit_search_filters(&owner, |f| {
                            f.archived = !f.archived
                        })),
                );
                if tab.has_chat_type() {
                    menu = menu.separator();
                    for kind in SearchChatType::ALL
                        .into_iter()
                        .filter(|kind| *kind != SearchChatType::Channels)
                    {
                        menu = menu.item(
                            PopupMenuItem::new(kind.label())
                                .checked(filters.chat_type == kind)
                                .on_click(Self::edit_search_filters(&owner, move |f| {
                                    f.chat_type = kind
                                })),
                        );
                    }
                }
                menu
            },
        )
    }

    /// "Any time ▾": the date window of `searchMessages`.
    pub(super) fn search_date_menu(
        filters: GlobalSearchFilters,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let narrowed = filters.date != SearchDateRange::default();
        Self::filter_menu_button("search-filter-date", filters.date.label().into(), narrowed)
            .dropdown_menu(move |mut menu: PopupMenu, _, _| {
                for range in SearchDateRange::ALL {
                    menu = menu.item(
                        PopupMenuItem::new(range.label())
                            .checked(filters.date == range)
                            .on_click(Self::edit_search_filters(&owner, move |f| f.date = range)),
                    );
                }
                menu
            })
    }
}

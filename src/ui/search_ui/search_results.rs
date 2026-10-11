//! Methods moved out of `search_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.search.search.status)
            .unwrap_or(SearchStatus::Closed);
        let recents = session.is_some_and(|s| s.search.search.recents);
        let query = session
            .map(|s| s.search.search.query.clone())
            .unwrap_or_default();
        let chat_ids: Vec<ChatId> = session
            .map(|s| s.search.search.merged_chat_ids())
            .unwrap_or_default();
        let tab = session.map_or(SearchTab::Chats, |s| {
            SearchTab::of(&s.search.search.filters)
        });
        let public_scope = session
            .is_some_and(|s| s.search.search.filters.scope == SearchScope::PublicPosts && !recents);
        let apps_scope = tab == SearchTab::Apps;
        // The Channels tab lists its channels until a query is typed; the
        // media tabs list their kind of message from every chat.
        let channels_idle = tab == SearchTab::Channels && query.trim().is_empty();
        // Photos, Videos and Voice list everything whatever is typed.
        let browsing_media = tab.is_media() && (query.trim().is_empty() || !tab.takes_query());
        let show_chats = matches!(tab, SearchTab::Chats | SearchTab::Channels) && !channels_idle;
        let show_messages = !matches!(tab, SearchTab::Apps | SearchTab::Downloads)
            && !channels_idle
            && !(tab == SearchTab::Posts && query.trim().is_empty());
        let limits_exceeded = session.is_some_and(|s| s.search.search.public_limits_exceeded);
        let messages: Vec<(ChatId, MessageId, String, String, i32)> = session
            .map(|s| {
                s.search
                    .search
                    .messages
                    .iter()
                    .map(|hit| {
                        let title = s
                            .chats
                            .get(&hit.chat_id.0)
                            .map(|c| c.title.clone())
                            .unwrap_or_else(|| format!("chat {}", hit.chat_id.0));
                        (
                            hit.chat_id,
                            hit.message_id,
                            title,
                            hit.preview.clone(),
                            hit.date,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                chat_ids
                    .into_iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Phase 7.2: `searchPublicChats` hits (public username/title lookup).
        // TDLib excludes known chats from these results, so they are shown
        // as their own section rather than merged into `Chats`.
        let public_chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                s.search
                    .search
                    .public_only_chat_ids()
                    .iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (*id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let only_channels = |rows: Vec<(ChatId, String, String)>| -> Vec<(ChatId, String, String)> {
            if tab != SearchTab::Channels {
                return rows;
            }
            rows.into_iter()
                .filter(|(id, ..)| {
                    session
                        .and_then(|s| s.chats.get(&id.0))
                        .is_some_and(|chat| chat.is_channel())
                })
                .collect()
        };
        let chats = only_channels(chats);
        let public_chats = only_channels(public_chats);
        let has_results = !chats.is_empty() || !public_chats.is_empty() || !messages.is_empty();
        // Only states the results don't already show: still loading with
        // nothing yet, nothing found, failure.
        let hint = match status {
            _ if apps_scope || channels_idle || tab == SearchTab::Downloads => String::new(),
            _ if tab == SearchTab::Posts && query.trim().is_empty() => tab.idle_hint().to_string(),
            SearchStatus::Idle if tab != SearchTab::Chats => tab.idle_hint().to_string(),
            SearchStatus::Idle if recents && has_results => String::new(),
            SearchStatus::Idle => tab.idle_hint().to_string(),
            SearchStatus::Searching if has_results => String::new(),
            SearchStatus::Searching if recents => "Loading recent chats…".to_string(),
            SearchStatus::Searching if browsing_media => "Loading…".to_string(),
            SearchStatus::Searching => format!("Searching “{query}”…"),
            SearchStatus::Empty if browsing_media => tab.idle_hint().to_string(),
            SearchStatus::Empty if tab == SearchTab::Channels => {
                format!("No channels match “{query}”.")
            }
            SearchStatus::Empty if tab.is_media() => {
                format!("Nothing matches “{query}”.")
            }
            SearchStatus::Ready => String::new(),
            SearchStatus::Empty if public_scope && limits_exceeded => {
                "The free daily limit for searching public posts is used up.".to_string()
            }
            SearchStatus::Empty if public_scope => {
                format!("No public posts match “{query}”.")
            }
            SearchStatus::Empty => format!("No chats or messages match “{query}”."),
            SearchStatus::Failed => "Search failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        let chat_heading = if recents { "Recent" } else { "Chats" };
        let message_heading = if public_scope {
            "Public posts"
        } else {
            "Messages"
        };
        let confirm = session.and_then(|s| s.search.search.confirm);
        let messages_empty = messages.is_empty();
        // A new query, tab or filter starts the list from its top.
        if let Some(search) = session.map(|s| &s.search.search) {
            if self.search_ui.seen_generation.get() != search.generation {
                self.search_ui.seen_generation.set(search.generation);
                self.search_ui
                    .results_scroll
                    .set_offset(point(px(0.), px(0.)));
            }
            // Infinite scroll: look again once this frame is laid out, so a
            // page that left the list short of the viewport asks for more.
            let tabs = &search.tabs;
            if tabs.has_more() && !tabs.loading_more && !tabs.more_failed {
                let entity = cx.entity();
                cx.defer(move |cx| {
                    entity.update(cx, |this, cx| this.maybe_load_more_search(cx));
                });
            }
        }
        let bar = self.search_filter_bar(cx);
        let body = div()
            .id("search-results-body")
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.search_ui.results_scroll)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                if this
                    .session()
                    .is_some_and(|s| s.search.search.tabs.has_more())
                {
                    this.notify_sidebar(cx);
                }
            }))
            .when(tab == SearchTab::Posts, |this| {
                this.children(self.posts_quota_line(cx))
                    .children(self.posts_paid_block(cx))
            })
            .when(tab == SearchTab::Downloads, |this| {
                this.child(self.search_downloads_block(&query, cx))
            })
            .when(channels_idle, |this| {
                this.children(self.channels_tab_block(cx))
            })
            .when(apps_scope, |this| {
                this.child(self.apps_tab_block(&query, cx))
            })
            .when_some(
                (recents && tab == SearchTab::Chats)
                    .then(|| self.frequent_contacts(cx))
                    .flatten(),
                |this, strip| this.child(strip),
            )
            .when_some(confirm, |this, confirm| {
                this.child(self.search_confirm_row(confirm, cx))
            })
            .when_some(self.search_community_filter_chips(cx), |this, chips| {
                this.child(chips)
            })
            .when(!hint.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                )
            })
            .when(!chats.is_empty() && show_chats, |this| {
                let mut block = div().id("search-chats").flex().flex_col().gap_1().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_xs().font_semibold().child(chat_heading))
                        // Slice CL2: "Clear" on the Recent heading —
                        // `clearRecentlyFoundChats` (TGX
                        // `SearchManager.clearRecentlyFoundChats`).
                        .when(recents, |this| {
                            this.child(
                                Button::new("clear-search-recents")
                                    .label("Clear")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_search_prompt(
                                            Some(SearchConfirm::ClearRecents),
                                            None,
                                            cx,
                                        );
                                    })),
                            )
                        }),
                );
                for (id, title, preview) in chats {
                    let photo = self.chat_photo_for_row(id);
                    let row_title = title.clone();
                    let row = search_result_row(
                        ("search-chat", id.0 as u64),
                        title,
                        preview,
                        None,
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    );
                    block = block.child(if recents {
                        // tdesktop: "Remove from Recent" on each entry.
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().flex_1().min_w_0().child(row))
                            .child(
                                Button::new(("search-recent-remove", id.0 as u64))
                                    .icon(gpui_kit::assets::IconName::X)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Remove from Recent")
                                    .accessibility_label(format!("Remove {row_title} from Recent"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_recent_search(id, cx);
                                    })),
                            )
                            .into_any_element()
                    } else {
                        row.into_any_element()
                    });
                }
                this.child(block)
            })
            .when_some(self.story_search_section(&query, cx), |this, section| {
                this.child(section)
            })
            .when(!public_chats.is_empty() && show_chats, |this| {
                let mut block = div()
                    .id("search-public-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Public chats"));
                for (id, title, preview) in public_chats {
                    let photo = self.chat_photo_for_row(id);
                    block = block.child(search_result_row(
                        ("search-public-chat", id.0 as u64),
                        title,
                        preview,
                        None,
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    ));
                }
                this.child(block)
            })
            .when(!messages.is_empty() && show_messages, |this| {
                if let Some(media) = tab.media_tab() {
                    return this.children(self.search_media_block(media, cx));
                }
                let mut block = div()
                    .id("search-messages")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child(message_heading));
                for (chat_id, message_id, title, preview, date) in messages {
                    let photo = self.chat_photo_for_row(chat_id);
                    block = block.child(search_result_row(
                        ("search-msg", message_id.0 as u64),
                        title,
                        preview,
                        Some(date),
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| {
                            this.select_search_message(chat_id, message_id, window, cx);
                        },
                    ));
                }
                this.child(block)
            })
            .when(!messages_empty && show_messages, |this| {
                this.children(self.search_load_more_row(cx))
            });
        div()
            .id("search-results")
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h_0()
            .children(bar)
            .child(body)
    }
}

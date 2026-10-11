//! What the tabs of the global search list below the strip: the Channels
//! tab's joined and similar channels, the media tabs' rows (the
//! shared-media list's anatomy, across every chat), the Posts tab's quota
//! and the "Load more" row of a paged list.

use super::*;
use quill::state::{SearchMediaRow, SharedMediaTab, media_month_starts};

/// tdesktop `kCollapsedChannelsCount`.
const COLLAPSED_CHANNELS: usize = 5;

impl QuillApp {
    /// Channels tab on an empty field: the channels you joined (five, then
    /// "Show more") and the recommended ones (`getRecommendedChats`).
    pub(super) fn channels_tab_block(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let mut joined: Vec<_> = session
            .chats
            .values()
            .filter(|chat| chat.is_channel() && chat.in_main_list)
            .collect();
        joined.sort_by_key(|chat| std::cmp::Reverse(chat.order));
        let total = joined.len();
        let expanded = self.search_ui.channels_expanded;
        let shown = if expanded {
            total
        } else {
            total.min(COLLAPSED_CHANNELS)
        };
        let joined_rows: Vec<(ChatId, String, String)> = joined
            .iter()
            .take(shown)
            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
            .collect();
        let tabs = &session.search.search.tabs;
        let recommended: Vec<(ChatId, String, String)> = tabs
            .recommended
            .iter()
            .filter(|id| !joined.iter().any(|chat| chat.id == **id))
            .filter_map(|id| session.chats.get(&id.0))
            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
            .collect();
        let loading = tabs.recommended_loading;
        let muted = cx.theme().muted_foreground;
        let section = |id: &'static str,
                       prefix: &'static str,
                       heading: &str,
                       rows: Vec<(ChatId, String, String)>,
                       cx: &mut Context<Self>| {
            let mut block = div()
                .id(id)
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().font_semibold().child(heading.to_string()));
            for (chat_id, title, preview) in rows {
                let photo = self.chat_photo_for_row(chat_id);
                block = block.child(search_result_row(
                    (prefix, chat_id.0 as u64),
                    title,
                    preview,
                    None,
                    "",
                    photo,
                    cx,
                    move |this, window, cx| this.select_search_chat(chat_id, window, cx),
                ));
            }
            block
        };
        let mut tab = div().id("search-channels").flex().flex_col().gap_2();
        if !joined_rows.is_empty() {
            let mut block = section(
                "search-channels-joined",
                "search-channel",
                "Channels you joined",
                joined_rows,
                cx,
            );
            if total > COLLAPSED_CHANNELS {
                block = block.child(
                    div().flex().child(
                        Button::new("search-channels-more")
                            .label(if expanded { "Show less" } else { "Show more" })
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.search_ui.channels_expanded =
                                    !this.search_ui.channels_expanded;
                                cx.notify();
                            })),
                    ),
                );
            }
            tab = tab.child(block);
        }
        if !recommended.is_empty() {
            tab = tab.child(section(
                "search-channels-similar",
                "search-channel-similar",
                "Similar channels",
                recommended,
                cx,
            ));
        } else if loading {
            tab = tab.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Loading similar channels…"),
            );
        } else if total == 0 {
            tab = tab.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Type to search channels."),
            );
        }
        Some(tab.into_any_element())
    }

    /// The hits of a media tab as rows under month headers, like the
    /// shared-media list; a row opens the message in its chat.
    pub(super) fn search_media_block(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let rows = session.search.search.media_rows(tab);
        if rows.is_empty() {
            return None;
        }
        let starts = media_month_starts(&rows);
        let muted = cx.theme().muted_foreground;
        let mut block = div().id("search-media").flex().flex_col();
        for (index, row) in rows.into_iter().enumerate() {
            if let Some((_, month)) = starts.iter().find(|(start, _)| *start == index) {
                block = block.child(
                    div()
                        .id(("search-media-month", index as u64))
                        .role(gpui_kit::Role::Heading)
                        .aria_label(month.title())
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(month.title()),
                );
            }
            let chat_title = session
                .chats
                .get(&row.chat_id.0)
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("chat {}", row.chat_id.0));
            block = block.child(self.search_media_row(row, chat_title, cx));
        }
        Some(block.into_any_element())
    }

    fn search_media_row(
        &self,
        row: SearchMediaRow,
        chat_title: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let SearchMediaRow {
            chat_id,
            message_id,
            glyph,
            label,
            date,
        } = row;
        let stamp = (date > 0).then(|| {
            let now = quill::local_time::civil_local(quill::local_time::now_unix());
            quill::local_time::chat_list_stamp(
                &quill::local_time::civil_local(i64::from(date)),
                &now,
            )
        });
        let subtitle = match stamp {
            Some(stamp) => format!("{chat_title} · {stamp}"),
            None => chat_title.clone(),
        };
        let label = one_line_preview(&label);
        div()
            .id(("search-media-item", message_id.0 as u64))
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded_md()
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{label} · {chat_title}"))
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_search_message(chat_id, message_id, window, cx);
            }))
            .child(
                div()
                    .w(px(40.))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .text_lg()
                    .child(glyph),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().truncate().font_medium().child(label))
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::ui::bidi_line::one_line_plain(subtitle)),
                    ),
            )
    }

    /// The Posts tab's "N free searches left today".
    pub(super) fn posts_quota_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let limits = self.session()?.search.search.tabs.limits?;
        Some(
            div()
                .id("search-posts-quota")
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(limits.quota_line())
                .into_any_element(),
        )
    }

    /// "Load more" under a paged list (media rows, messages, posts); says
    /// so when the last page failed.
    pub(super) fn search_load_more_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tabs = &self.session()?.search.search.tabs;
        if !tabs.has_more() {
            return None;
        }
        let label = if tabs.loading_more {
            "Loading…"
        } else if tabs.more_failed {
            "Could not load more. Try again"
        } else {
            "Load more"
        };
        let busy = tabs.loading_more;
        Some(
            div()
                .flex()
                .justify_center()
                .py_1()
                .child(
                    Button::new("search-load-more")
                        .label(label)
                        .ghost()
                        .disabled(busy)
                        .accessibility_label("Load more search results")
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_search_ui(cx))),
                )
                .into_any_element(),
        )
    }

    pub(super) fn load_more_search_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.load_more_search_results().is_err()
        {
            self.connection.status_note = "could not load more results".into();
        }
        cx.notify();
    }
}

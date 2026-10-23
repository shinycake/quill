//! Story statistics with public shares, public story search, and the
//! topic / thread info cards.
//!
//! tdesktop: `info/statistics/info_statistics_inner_widget.cpp` (story
//! statistics: overview, two charts, "Public Shares" list), the story
//! hashtag / location search, `info/profile` topic details.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::chat_theme::{accent, bg_canvas, border, danger, text_menu, text_muted};
use super::scroll_load::{load_due, loading_row, scrolled_near_end, watch_scroll};
use super::statistics::format_stat_count;
use super::statistics_graph::stat_graph_block;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::stat_chart::ChartKind;
use quill::state::{
    StorySearchQuery, StoryStatsFetch, event_log_relative_time, private_share_count,
    story_search_tag,
};
use quill::story_viewer::collect_found_story_items;
use quill::telegram::envelope::{PublicForwardKind, StoryAreaKind};
use quill::topic_info::InfoRow;

impl QuillApp {
    /// The "Statistics" button toggles the panel and loads both answers.
    pub(super) fn toggle_story_stats(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        self.stories.stats_open = !self.stories.stats_open;
        if self.stories.stats_open {
            self.stories.viewers_open = false;
            self.stories.report_open = false;
            self.fetch_story_stats(&item.chat_id, item.story_id, cx);
        }
        cx.notify();
    }

    fn fetch_story_stats(&mut self, chat_id: &ChatId, story_id: i32, cx: &mut Context<Self>) {
        let is_dark = super::chat_theme::is_dark_palette();
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .retry_story_statistics(*chat_id, story_id, is_dark)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_STORY_STATISTICS,
                )));
            }
        } else if self.demo_session.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_STORY_STATISTICS_RUN_WITH,
            )));
        }
        cx.notify();
    }

    /// The statistics panel is near its end: ask for the next public shares.
    fn story_forwards_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let current = self
            .stories
            .viewer
            .current()
            .map(|item| (item.chat_id, item.story_id));
        let Some(state) = self
            .session()
            .and_then(|s| s.stories.insights.as_ref())
            .filter(|state| current == Some((ChatId(state.chat_id), state.story_id)))
        else {
            return;
        };
        if !self.stories.stats_open
            || (state.forwards_error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.stories.stats_scroll),
                !state.forwards_next_offset.is_empty(),
                state.forwards_loading,
            )
            || !self
                .stories
                .stats_asked
                .allow(state.forwards.len(), scrolled)
        {
            return;
        }
        self.load_more_story_forwards(cx);
    }

    /// The public-story hits are near their end: ask for the next page.
    fn found_stories_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(search) = self.session().and_then(|s| s.stories.search.as_ref()) else {
            return;
        };
        if (search.error.is_some() && !scrolled)
            || !load_due(
                scrolled_near_end(&self.stories.found_scroll),
                !search.next_offset.is_empty(),
                search.loading,
            )
            || !self
                .stories
                .found_asked
                .allow(search.stories.len(), scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_found_stories();
        }
        cx.notify();
    }

    fn load_more_story_forwards(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let offset = self
            .session()
            .and_then(|s| s.stories.insights.as_ref())
            .map(|state| state.forwards_next_offset.clone())
            .unwrap_or_default();
        if offset.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_story_public_forwards(item.chat_id, item.story_id, &offset)
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_PUBLIC_SHARES,
            )));
        }
        cx.notify();
    }

    fn chat_title_or_id(&self, chat_id: i64) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| tr_with(keys::Q_MEDIA_CHAT_2, &[("chat_id", &chat_id.to_string())]))
    }

    fn stat_cell(label: &'static str, value: String) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_menu())
                    .child(value),
            )
            .child(div().text_xs().text_color(text_muted()).child(label))
    }

    /// tdesktop "Story Statistic": overview counters, the two charts and
    /// the public shares list.
    pub(super) fn story_stats_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .id("story-stats-panel")
            .flex()
            .flex_col()
            .gap_2()
            // Charts need a definite width.
            .w(px(360.))
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.stories.stats_scroll)
            .on_scroll_wheel(watch_scroll(cx, Self::story_forwards_more))
            .p_2()
            .rounded_md()
            .bg(bg_canvas())
            .border_1()
            .border_color(border())
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(text_menu())
                    .child(tr(keys::Q_MEDIA_STORY_STATISTICS)),
            );
        let current = self
            .stories
            .viewer
            .current()
            .map(|i| (i.chat_id, i.story_id));
        let state = self
            .session()
            .and_then(|s| s.stories.insights.clone())
            .filter(|state| current == Some((ChatId(state.chat_id), state.story_id)));
        let Some(state) = state else {
            return panel
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_LOADING_STATISTICS)),
                )
                .into_any_element();
        };
        let info = self.current_story().and_then(|s| s.interaction_info);
        if let Some(info) = info {
            panel = panel
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Self::stat_cell(
                            tr(keys::Q_MEDIA_VIEWS),
                            format_stat_count(f64::from(info.view_count)),
                        ))
                        .child(Self::stat_cell(
                            tr(keys::Q_MEDIA_PUBLIC_SHARES),
                            format_stat_count(f64::from(state.forwards_total)),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Self::stat_cell(
                            tr(keys::Q_MEDIA_REACTIONS),
                            format_stat_count(f64::from(info.reaction_count)),
                        ))
                        .child(Self::stat_cell(
                            tr(keys::Q_MEDIA_PRIVATE_SHARES),
                            format_stat_count(f64::from(private_share_count(
                                info.forward_count,
                                state.forwards_total,
                            ))),
                        )),
                );
        }
        match &state.statistics {
            StoryStatsFetch::Loading => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_LOADING_STATISTICS)),
                );
            }
            StoryStatsFetch::Failed(message) => {
                panel = panel
                    .child(div().text_sm().text_color(danger()).child(message.clone()))
                    .child(
                        Button::new("story-stats-retry")
                            .label(tr(keys::Q_MEDIA_RETRY))
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(item) = this.stories.viewer.current().cloned() {
                                    this.fetch_story_stats(&item.chat_id, item.story_id, cx);
                                }
                            })),
                    );
            }
            StoryStatsFetch::Loaded(stats) => {
                // tdesktop draws these as a two-scale line chart and a
                // step chart.
                for (id, label, graph, kind) in [
                    (
                        "story-viewer-interactions",
                        tr(keys::Q_MEDIA_INTERACTIONS),
                        &stats.interaction_graph,
                        ChartKind::DoubleLinear,
                    ),
                    (
                        "story-viewer-reactions",
                        tr(keys::Q_MEDIA_REACTIONS),
                        &stats.reaction_graph,
                        ChartKind::Bar,
                    ),
                ] {
                    if let Some(row) = stat_graph_block(id, label, graph, kind) {
                        panel = panel.child(row);
                    }
                }
            }
        }
        panel = panel.child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(text_muted())
                .child(tr_with(
                    keys::Q_MEDIA_PUBLIC_SHARES_2,
                    &[("forwards_total", &state.forwards_total.to_string())],
                )),
        );
        for (index, forward) in state.forwards.iter().enumerate() {
            let (title, detail) = match forward.kind {
                PublicForwardKind::Message { chat_id, .. } => {
                    (self.chat_title_or_id(chat_id), tr(keys::Q_MEDIA_MESSAGE))
                }
                PublicForwardKind::Story { chat_id, .. } => (
                    self.chat_title_or_id(chat_id),
                    tr(keys::Q_MEDIA_STORY_REPOST),
                ),
            };
            let kind = forward.kind;
            let row = div()
                .id(("story-forward", index as u64))
                .flex()
                .gap_2()
                .items_center()
                .child(initials_avatar(&title, 28.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .text_color(text_menu())
                                .truncate()
                                .child(super::bidi_line::one_line_plain(title)),
                        )
                        .child(div().text_xs().text_color(text_muted()).child(format!(
                            "{detail} · {}",
                            event_log_relative_time(forward.date)
                        ))),
                );
            panel = panel.child(match kind {
                PublicForwardKind::Message {
                    chat_id,
                    message_id,
                } => row
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.story_area_open_message(
                            ChatId(chat_id),
                            quill::ids::MessageId(message_id),
                            cx,
                        );
                    }))
                    .into_any_element(),
                PublicForwardKind::Story { .. } => row.into_any_element(),
            });
        }
        if let Some(error) = state.forwards_error.clone() {
            panel = panel.child(div().text_sm().text_color(danger()).child(error));
        }
        if state.forwards_loading {
            panel = panel.child(loading_row("story-forwards-loading", cx));
        } else if state.forwards.is_empty()
            && state.forwards_next_offset.is_empty()
            && matches!(state.statistics, StoryStatsFetch::Loaded(_))
        {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_NO_PUBLIC_SHARES_YET)),
            );
        }
        panel.into_any_element()
    }

    /// The location or venue the current story is tagged with, as a search.
    pub(super) fn viewer_story_search_query(&self) -> Option<StorySearchQuery> {
        let item = self.stories.viewer.current()?;
        item.areas.iter().find_map(|area| match &area.kind {
            StoryAreaKind::Venue {
                title,
                provider,
                venue_id,
                ..
            } if !provider.is_empty() && !venue_id.is_empty() => Some(StorySearchQuery::Venue {
                provider: provider.clone(),
                venue_id: venue_id.clone(),
                label: title.clone(),
            }),
            StoryAreaKind::Location {
                address,
                address_parts,
                ..
            } if !address.is_empty() => Some(StorySearchQuery::Location {
                address: address_parts.clone(),
                label: address.clone(),
            }),
            _ => None,
        })
    }

    /// Start a public story search; the results show in the sidebar search.
    pub(super) fn search_public_stories_ui(
        &mut self,
        query: StorySearchQuery,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.search_public_stories(query).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_SEARCH_STORIES,
                )));
            }
        } else if self.demo_session.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_PUBLIC_STORY_SEARCH_RUNS,
            )));
        }
        cx.notify();
    }

    /// "Stories here" in the viewer: search the story's venue or location,
    /// then show the hits in the sidebar search.
    pub(super) fn search_stories_from_viewer(
        &mut self,
        query: StorySearchQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_story_viewer(cx);
        self.open_search_ui(window, cx);
        self.search_public_stories_ui(query, cx);
    }

    fn open_found_story(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(keys) = self
            .session()
            .and_then(|s| s.stories.search.as_ref())
            .map(|search| search.stories.clone())
        else {
            return;
        };
        let items = self
            .session()
            .map(|s| collect_found_story_items(&keys, &s.stories.stories))
            .unwrap_or_default();
        let Some(target) = keys.get(index) else {
            return;
        };
        let Some(position) = items
            .iter()
            .position(|item| (item.chat_id.0, item.story_id) == *target)
        else {
            return;
        };
        self.begin_story_viewer(items, position, cx);
        cx.notify();
    }

    /// Public stories section of the sidebar search: a prompt row for a
    /// typed `#tag` / `$tag`, and the hits of the running or last search.
    pub(super) fn story_search_section(
        &self,
        typed: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tag = story_search_tag(typed);
        let search = self.session().and_then(|s| s.stories.search.clone());
        let showing = search.as_ref().filter(|search| match &search.query {
            StorySearchQuery::Tag(active) => tag.as_deref() == Some(active.as_str()),
            _ => true,
        });
        if tag.is_none() && showing.is_none() {
            return None;
        }
        let mut block = div()
            .id("search-public-stories")
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .child(tr(keys::Q_MEDIA_PUBLIC_STORIES)),
            );
        let Some(search) = showing else {
            let tag = tag?;
            let label = tr_with(
                keys::Q_MEDIA_SEARCH_PUBLIC_STORIES_FOR,
                &[("tag", &tag.to_string())],
            );
            return Some(
                block
                    .child(
                        Button::new("search-public-stories-go")
                            .label(label)
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.search_public_stories_ui(
                                    StorySearchQuery::Tag(tag.clone()),
                                    cx,
                                );
                            })),
                    )
                    .into_any_element(),
            );
        };
        block = block.child(div().text_xs().text_color(text_muted()).child(tr_with(
            keys::Q_MEDIA_STORIES_FOR,
            &[("label", (search.query.label()))],
        )));
        let mut rows = div()
            .id("found-stories-rows")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(260.))
            .overflow_y_scroll()
            .track_scroll(&self.stories.found_scroll)
            .on_scroll_wheel(watch_scroll(cx, Self::found_stories_more));
        for (index, (chat_id, story_id)) in search.stories.iter().enumerate() {
            let name = self.chat_title_or_id(*chat_id);
            let date = self
                .session()
                .and_then(|s| s.stories.stories.get(&(*chat_id, *story_id)))
                .map(|story| story.date)
                .unwrap_or(0);
            rows = rows.child(
                div()
                    .id(("found-story", index as u64))
                    .flex()
                    .gap_2()
                    .items_center()
                    .p_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(accent().opacity(0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_found_story(index, cx)))
                    .child(initials_avatar(&name, 32.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(name)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted())
                                    .child(event_log_relative_time(date)),
                            ),
                    ),
            );
        }
        if search.loading && !search.stories.is_empty() {
            rows = rows.child(loading_row("found-stories-loading", cx));
        }
        block = block.child(rows);
        if let Some(error) = search.error.clone() {
            block = block.child(div().text_sm().text_color(danger()).child(error));
        }
        if search.loading && search.stories.is_empty() {
            block = block.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_SEARCHING_STORIES)),
            );
        } else if search.stories.is_empty() && !search.loading && search.error.is_none() {
            block = block.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_NO_PUBLIC_STORIES_FOUND)),
            );
        }
        Some(block.into_any_element())
    }

    /// A label/value info card shared by the topic and thread headers.
    pub(super) fn info_card(
        &self,
        id: &'static str,
        rows: Vec<InfoRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut card = div()
            .id(id)
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(bg_canvas());
        if rows.is_empty() {
            card = card.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_NO_FURTHER_DETAILS)),
            );
        }
        for row in rows {
            card = card.child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .child(div().text_sm().text_color(text_muted()).child(row.label))
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_menu())
                            .truncate()
                            .child(row.value),
                    ),
            );
        }
        card.into_any_element()
    }

    fn sender_name(&self, sender: &quill::telegram::envelope::MessageSender) -> String {
        self.story_viewer_actor_name(sender)
    }

    fn stamp(unix: i32) -> Option<String> {
        (unix > 0).then(|| {
            quill::local_time::full_stamp(&quill::local_time::civil_local(i64::from(unix)))
        })
    }

    /// Rows of the open forum topic's info card.
    pub(super) fn topic_info_card(
        &self,
        topic: &quill::telegram::envelope::ForumTopic,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let creator = topic.creator.as_ref().map(|c| self.sender_name(c));
        let rows = quill::topic_info::topic_info_rows(
            topic,
            creator.as_deref(),
            Self::stamp(topic.creation_date).as_deref(),
        );
        self.info_card("forum-topic-info", rows, cx)
    }

    /// Rows of the open reply thread's info card.
    pub(super) fn thread_info_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let thread = session.thread_for_chat(session.open_chat?)?;
        let root = thread.root_message();
        let started_by = root
            .and_then(|m| m.sender.as_ref())
            .map(|sender| self.sender_name(sender));
        let started = root.and_then(|m| Self::stamp(m.date));
        let rows = quill::topic_info::thread_info_rows(
            thread.is_comments(),
            thread.reply_count,
            thread.unread_count,
            started_by.as_deref(),
            started.as_deref(),
        );
        Some(self.info_card("thread-info", rows, cx))
    }
}

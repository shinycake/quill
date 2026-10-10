//! Story statistics with public shares, public story search, and the
//! topic / thread info cards.
//!
//! tdesktop: `info/statistics/info_statistics_inner_widget.cpp` (story
//! statistics: overview, two charts, "Public Shares" list), the story
//! hashtag / location search, `info/profile` topic details.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::chat_theme::{accent, bg_canvas, border, danger, text_menu, text_muted};
use super::statistics::{format_stat_count, stats_graph_row};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
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
                self.connection.status_note = "could not load story statistics".into();
            }
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — story statistics run with live TDLib".into();
        }
        cx.notify();
    }

    fn load_more_story_forwards(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let offset = self
            .session()
            .and_then(|s| s.story_insights.as_ref())
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
            self.connection.status_note = "could not load public shares".into();
        }
        cx.notify();
    }

    fn chat_title_or_id(&self, chat_id: i64) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("Chat {chat_id}"))
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
            .max_w(px(360.))
            .max_h(px(320.))
            .overflow_y_scroll()
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
                    .child("Story statistics"),
            );
        let current = self
            .stories
            .viewer
            .current()
            .map(|i| (i.chat_id, i.story_id));
        let state = self
            .session()
            .and_then(|s| s.story_insights.clone())
            .filter(|state| current == Some((ChatId(state.chat_id), state.story_id)));
        let Some(state) = state else {
            return panel
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Loading statistics…"),
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
                            "Views",
                            format_stat_count(f64::from(info.view_count)),
                        ))
                        .child(Self::stat_cell(
                            "Public shares",
                            format_stat_count(f64::from(state.forwards_total)),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Self::stat_cell(
                            "Reactions",
                            format_stat_count(f64::from(info.reaction_count)),
                        ))
                        .child(Self::stat_cell(
                            "Private shares",
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
                        .child("Loading statistics…"),
                );
            }
            StoryStatsFetch::Failed(message) => {
                panel = panel
                    .child(div().text_sm().text_color(danger()).child(message.clone()))
                    .child(
                        Button::new("story-stats-retry")
                            .label("Retry")
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(item) = this.stories.viewer.current().cloned() {
                                    this.fetch_story_stats(&item.chat_id, item.story_id, cx);
                                }
                            })),
                    );
            }
            StoryStatsFetch::Loaded(stats) => {
                for (label, graph) in [
                    ("Story interactions", &stats.interaction_graph),
                    ("Story reactions", &stats.reaction_graph),
                ] {
                    if let Some(row) = stats_graph_row(label, graph, cx) {
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
                .child(format!("Public shares ({})", state.forwards_total)),
        );
        for (index, forward) in state.forwards.iter().enumerate() {
            let (title, detail) = match forward.kind {
                PublicForwardKind::Message { chat_id, .. } => {
                    (self.chat_title_or_id(chat_id), "Message")
                }
                PublicForwardKind::Story { chat_id, .. } => {
                    (self.chat_title_or_id(chat_id), "Story repost")
                }
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
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("Loading public shares…"),
            );
        } else if !state.forwards_next_offset.is_empty() {
            panel = panel.child(
                Button::new("story-forwards-more")
                    .label("Load more")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.load_more_story_forwards(cx))),
            );
        } else if state.forwards.is_empty()
            && matches!(state.statistics, StoryStatsFetch::Loaded(_))
        {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("No public shares yet"),
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
                self.connection.status_note = "could not search stories".into();
            }
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — public story search runs with live TDLib".into();
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
            .and_then(|s| s.story_search.as_ref())
            .map(|search| search.stories.clone())
        else {
            return;
        };
        let items = self
            .session()
            .map(|s| collect_found_story_items(&keys, &s.stories))
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
        let search = self.session().and_then(|s| s.story_search.clone());
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
            .child(div().text_xs().font_semibold().child("Public stories"));
        let Some(search) = showing else {
            let tag = tag?;
            let label = format!("Search public stories for {tag}");
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
        block = block.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(format!("Stories for {}", search.query.label())),
        );
        for (index, (chat_id, story_id)) in search.stories.iter().enumerate() {
            let name = self.chat_title_or_id(*chat_id);
            let date = self
                .session()
                .and_then(|s| s.stories.get(&(*chat_id, *story_id)))
                .map(|story| story.date)
                .unwrap_or(0);
            block = block.child(
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
        if let Some(error) = search.error.clone() {
            block = block.child(div().text_sm().text_color(danger()).child(error));
        }
        if search.loading {
            block = block.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("Searching stories…"),
            );
        } else if !search.next_offset.is_empty() {
            block = block.child(
                Button::new("found-stories-more")
                    .label("Load more")
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.load_more_found_stories();
                        }
                        cx.notify();
                    })),
            );
        } else if search.stories.is_empty() && search.error.is_none() {
            block = block.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("No public stories found"),
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
                    .child("No further details"),
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

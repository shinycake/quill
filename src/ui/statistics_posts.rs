//! Recent posts of a channel's statistics and the pages they open:
//! message and story statistics with their public shares (tdesktop
//! `info_statistics_inner_widget.cpp` `fillRecentPosts`, message and
//! story statistics; `info_statistics_list_controllers.cpp` public
//! forwards).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::chat_theme::{danger, text_muted};
use super::statistics::{divider, row_text, section_title, stats_row};
use super::statistics_graph::{OverviewCell, overview_grid};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::FileId;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::local_time::{civil_local, full_stamp};
use quill::stat_chart::{ChartKind, format_short};
use quill::state::story_thumb_candidates;
use quill::state::{
    ChatStatisticsFetch, MessageStatsFetch, PollStatsFetch, PostLookup, StatsPage, StoryStatsFetch,
    private_share_count,
};
use quill::telegram::envelope::{
    ChannelStatistics, ChatStatistics, ChatStatisticsObject, PublicForwardKind, PublicForwardView,
};

fn stamp(date: i32) -> String {
    if date > 0 {
        full_stamp(&civil_local(i64::from(date)))
    } else {
        String::new()
    }
}

/// Fetch a recent post's preview once (the driver skips repeats).
fn request_post(chat_id: i64, message_id: i64, cx: &mut Context<QuillApp>) {
    let weak = cx.weak_entity();
    cx.defer(move |cx| {
        let _ = weak.update(cx, |this, cx| {
            if let Some(live) = this.live.as_mut()
                && matches!(
                    live.driver
                        .fetch_stats_post(ChatId(chat_id), MessageId(message_id)),
                    Ok(Some(_))
                )
            {
                cx.notify();
            }
        });
    });
}

/// Ask once for a poll's votes chart.
fn request_poll_votes(chat_id: i64, message_id: i64, cx: &mut Context<QuillApp>) {
    let weak = cx.weak_entity();
    cx.defer(move |cx| {
        let _ = weak.update(cx, |this, cx| {
            let is_dark = super::chat_theme::is_dark_palette();
            if let Some(live) = this.live.as_mut()
                && matches!(
                    live.driver.fetch_stats_poll_votes(
                        ChatId(chat_id),
                        MessageId(message_id),
                        is_dark
                    ),
                    Ok(Some(_))
                )
            {
                cx.notify();
            }
        });
    });
}

/// Views on the first line, reactions and shares under them (tdesktop
/// `MessagePreview::paintEvent`).
fn post_counts(views: i32, forwards: i32, reactions: i32) -> Div {
    let small = |icon: IconName, count: i32| {
        div()
            .flex()
            .items_center()
            .gap_0p5()
            .child(Icon::new(icon).size(px(11.)))
            .child(format_short(i64::from(count)))
    };
    let mut bottom = div().flex().gap_2().text_color(text_muted());
    if reactions > 0 {
        bottom = bottom.child(small(IconName::Heart, reactions));
    }
    if forwards > 0 {
        bottom = bottom.child(small(IconName::Forward, forwards));
    }
    div()
        .flex()
        .flex_col()
        .items_end()
        .flex_none()
        .text_xs()
        .child(super::statistics::plural(views, "view", "views"))
        .child(bottom)
}

impl QuillApp {
    /// "Recent posts": ten first, thirty more per "Show more"; a row
    /// opens that post's or story's statistics.
    pub(super) fn recent_posts_section(
        &self,
        chat_id: i64,
        stats: &ChannelStatistics,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let total = stats.recent_interactions.len();
        if total == 0 {
            return None;
        }
        let shown = session.groups.stats.recent_posts_shown(chat_id, total);
        let mut list = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title(tr(keys::Q_MONEY_RECENT_POSTS)));
        let mut rows = 0;
        for (index, info) in stats.recent_interactions.iter().take(shown).enumerate() {
            let (text, date, open, thumbs): (String, i32, StatsPage, Vec<FileId>) =
                match info.object {
                    ChatStatisticsObject::Message { message_id } => {
                        match session.stats_post(chat_id, message_id) {
                            PostLookup::Ready(post) => (
                                post.text,
                                post.date,
                                StatsPage::Message(message_id),
                                post.thumbs,
                            ),
                            PostLookup::Missing => continue,
                            PostLookup::Unknown => {
                                request_post(chat_id, message_id, cx);
                                continue;
                            }
                        }
                    }
                    // tdesktop lists a story only when it is loaded.
                    ChatStatisticsObject::Story { story_id } => {
                        match session.stories.stories.get(&(chat_id, story_id)) {
                            Some(story) => (
                                if story.caption.is_empty() {
                                    tr(keys::Q_MONEY_STORY).to_string()
                                } else {
                                    story.caption.clone()
                                },
                                story.date,
                                StatsPage::Story(story_id),
                                story_thumb_candidates(story),
                            ),
                            None => continue,
                        }
                    }
                };
            rows += 1;
            let title = if text.is_empty() {
                tr(keys::Q_MONEY_POST).to_string()
            } else {
                text
            };
            list = list.child(
                stats_row(format!("recent-post-{index}").into(), cx)
                    .aria_label(SharedString::from(tr_with(
                        keys::Q_MONEY_STATISTICS_OF,
                        &[("title", &title.to_string())],
                    )))
                    .child(self.post_thumb(&thumbs, matches!(open, StatsPage::Story(_)), cx))
                    .child(row_text(title, stamp(date)))
                    .child(post_counts(
                        info.view_count,
                        info.forward_count,
                        info.reaction_count,
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| match open {
                        StatsPage::Message(message_id) => {
                            this.open_message_stats(chat_id, message_id, cx)
                        }
                        StatsPage::Story(story_id) => this.open_story_stats(chat_id, story_id, cx),
                        StatsPage::Overview => {}
                    })),
            );
        }
        if shown < total {
            list = list.child(
                Button::new("recent-posts-more")
                    .label(tr(keys::Q_MONEY_SHOW_MORE))
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(session) = this.stats_session() {
                            session.groups.stats.show_more_recent_posts(chat_id, total);
                        }
                        cx.notify();
                    })),
            );
        } else if rows == 0 {
            return None;
        }
        Some(list.into_any_element())
    }

    /// A post's picture, rounded, or a story's in a ring (tdesktop
    /// `MessagePreview`); the slot stays for text posts so rows align.
    fn post_thumb(&self, files: &[FileId], story: bool, cx: &App) -> AnyElement {
        const SIZE: f32 = 40.;
        let roots = self.media_display_roots();
        let path = self.session().and_then(|session| {
            files.iter().find_map(|id| {
                session
                    .media
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
            })
        });
        let slot = div().size(px(SIZE)).flex_none();
        let Some(path) = path else {
            return slot.into_any_element();
        };
        if story {
            slot.p_0p5()
                .rounded_full()
                .border_2()
                .border_color(cx.theme().primary)
                .child(
                    img(path)
                        .size_full()
                        .rounded_full()
                        .object_fit(ObjectFit::Cover),
                )
                .into_any_element()
        } else {
            slot.child(
                img(path)
                    .size_full()
                    .rounded_md()
                    .object_fit(ObjectFit::Cover),
            )
            .into_any_element()
        }
    }

    fn stats_session(&mut self) -> Option<&mut quill::state::Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// The message menu's "Statistics": the panel opens on the post.
    pub(super) fn open_message_statistics_panel(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_info_panel_target(
            quill::state::InfoPanelTarget::Statistics(chat_id.0),
            window,
            cx,
        );
        self.open_message_stats(chat_id.0, message_id.0, cx);
    }

    /// Open a post's statistics in the panel.
    pub(super) fn open_message_stats(
        &mut self,
        chat_id: i64,
        message_id: i64,
        cx: &mut Context<Self>,
    ) {
        let is_dark = super::chat_theme::is_dark_palette();
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .fetch_message_statistics(ChatId(chat_id), MessageId(message_id), is_dark)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULD_NOT_LOAD_STATISTICS,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_message_stats_page(chat_id, message_id);
        }
        cx.notify();
    }

    fn open_story_stats(&mut self, chat_id: i64, story_id: i32, cx: &mut Context<Self>) {
        let is_dark = super::chat_theme::is_dark_palette();
        if let Some(session) = self.stats_session() {
            session
                .groups
                .stats
                .set_page(chat_id, StatsPage::Story(story_id));
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_story_insights(ChatId(chat_id), story_id, is_dark)
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_LOAD_STORY_STATISTICS,
            )));
        }
        cx.notify();
    }

    fn stats_back(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(session) = self.stats_session() {
            session.groups.stats.set_page(chat_id, StatsPage::Overview);
        }
        cx.notify();
    }

    fn page_header(&self, chat_id: i64, title: &'static str, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("stats-back")
                    .icon(IconName::ChevronLeft)
                    .ghost()
                    .small()
                    .accessibility_label(tr(keys::Q_MONEY_BACK_TO_STATISTICS))
                    .tooltip(tr(keys::Q_MONEY_BACK_TO_STATISTICS))
                    .on_click(cx.listener(move |this, _, _, cx| this.stats_back(chat_id, cx))),
            )
            .child(section_title(title))
    }

    /// "Message statistics": the post, its numbers, two charts and the
    /// public shares.
    pub(super) fn message_stats_page(
        &self,
        chat_id: i64,
        message_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let mut page = div().flex().flex_col().gap_4().child(self.page_header(
            chat_id,
            tr(keys::Q_MONEY_MESSAGE_STATISTICS),
            cx,
        ));
        let post = match session.stats_post(chat_id, message_id) {
            PostLookup::Ready(post) => Some(post),
            PostLookup::Unknown => {
                request_post(chat_id, message_id, cx);
                None
            }
            PostLookup::Missing => None,
        };
        // The numbers come from the chat's recent interactions when the
        // post is among them, else from the post itself.
        let recent = match session.groups.chat_statistics.get(&chat_id) {
            Some(ChatStatisticsFetch::Loaded(stats)) => match stats.as_ref() {
                ChatStatistics::Channel(c) => c
                    .recent_interactions
                    .iter()
                    .find(|info| info.object == ChatStatisticsObject::Message { message_id }),
                ChatStatistics::Supergroup(_) => None,
            },
            _ => None,
        }
        .copied();
        let state = session.groups.stats.message_for(chat_id, message_id);
        if let Some(post) = post.as_ref() {
            let title = if post.text.is_empty() {
                tr(keys::Q_MONEY_POST).to_string()
            } else {
                post.text.clone()
            };
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(!post.thumbs.is_empty(), |row| {
                        row.child(self.post_thumb(&post.thumbs, false, cx))
                    })
                    .child(row_text(title, stamp(post.date)))
                    .child(
                        Button::new("stats-go-to-message")
                            .label(tr(keys::Q_MONEY_GO_TO_MESSAGE))
                            .small()
                            .outline()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.story_area_open_message(
                                    ChatId(chat_id),
                                    MessageId(message_id),
                                    cx,
                                );
                            })),
                    ),
            );
        }
        let views = recent
            .map(|r| r.view_count)
            .or(post.as_ref().map(|p| p.views));
        let forwards = recent
            .map(|r| r.forward_count)
            .or(post.as_ref().map(|p| p.forwards));
        let public = state.map_or(0, |s| s.forwards.total);
        let mut cells = Vec::new();
        if let Some(views) = views {
            cells.push(OverviewCell::plain(
                format_short(i64::from(views)),
                tr(keys::Q_MONEY_VIEWS),
            ));
        }
        cells.push(OverviewCell::plain(
            format_short(i64::from(public)),
            tr(keys::Q_MONEY_PUBLIC_SHARES),
        ));
        if let Some(reactions) = recent.map(|r| r.reaction_count) {
            cells.push(OverviewCell::plain(
                format_short(i64::from(reactions)),
                tr(keys::Q_MONEY_REACTIONS),
            ));
        }
        if let Some(forwards) = forwards {
            cells.push(OverviewCell::plain(
                format_short(i64::from(private_share_count(forwards, public))),
                tr(keys::Q_MONEY_PRIVATE_SHARES),
            ));
        }
        page = page.child(overview_grid(cells));
        match state.map(|s| &s.statistics) {
            None | Some(MessageStatsFetch::Loading) => {
                page = page.child(loading_line(tr(keys::Q_MONEY_LOADING_STATISTICS)));
            }
            Some(MessageStatsFetch::Failed(message)) => {
                page = page
                    .child(div().text_sm().text_color(danger()).child(message.clone()))
                    .child(
                        Button::new("message-stats-retry")
                            .label(tr(keys::Q_MONEY_RETRY))
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let is_dark = super::chat_theme::is_dark_palette();
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.retry_message_statistics(
                                        ChatId(chat_id),
                                        MessageId(message_id),
                                        is_dark,
                                    );
                                }
                                cx.notify();
                            })),
                    );
            }
            Some(MessageStatsFetch::Loaded(stats)) => {
                for (id, title, graph, kind) in [
                    (
                        "interactions",
                        tr(keys::Q_MONEY_INTERACTIONS),
                        &stats.interaction_graph,
                        ChartKind::DoubleLinear,
                    ),
                    (
                        "reactions",
                        tr(keys::Q_MONEY_REACTIONS),
                        &stats.reaction_graph,
                        ChartKind::Bar,
                    ),
                ] {
                    let id = format!("message-{chat_id}-{message_id}-{id}");
                    if let Some(section) =
                        self.stat_graph_section(chat_id, &id, title, graph, kind, cx)
                    {
                        page = page.child(divider(cx)).child(section);
                    }
                }
            }
        }
        // A poll adds its votes (tdesktop `fillPollVotesGraph`).
        if post.as_ref().is_some_and(|post| post.is_poll) {
            match session.messages.poll_stats.get(&(chat_id, message_id)) {
                None => request_poll_votes(chat_id, message_id, cx),
                Some(PollStatsFetch::Loaded(graph)) => {
                    let id = format!("message-{chat_id}-{message_id}-poll");
                    if let Some(section) = self.stat_graph_section(
                        chat_id,
                        &id,
                        tr(keys::Q_MONEY_POLL_VOTES),
                        graph,
                        ChartKind::Linear,
                        cx,
                    ) {
                        page = page.child(divider(cx)).child(section);
                    }
                }
                _ => {}
            }
        }
        if let Some(state) = state {
            let forwards = &state.forwards;
            let more = (!forwards.next_offset.is_empty()).then(|| {
                Button::new("message-forwards-more")
                    .label(tr(keys::Q_MONEY_SHOW_MORE))
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.load_more_message_forwards();
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            });
            page = page.child(self.public_shares(
                "message-forward",
                forwards.total,
                &forwards.forwards,
                forwards.loading,
                forwards.error.clone(),
                more,
                cx,
            ));
        }
        if matches!(
            state.map(|s| &s.statistics),
            Some(MessageStatsFetch::Loaded(_))
        ) {
            page = page.child(super::statistics_export::export_button(chat_id, cx));
        }
        page.into_any_element()
    }

    /// "Story statistics", opened from a recent story.
    pub(super) fn story_stats_page(
        &self,
        chat_id: i64,
        story_id: i32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let mut page = div().flex().flex_col().gap_4().child(self.page_header(
            chat_id,
            tr(keys::Q_MONEY_STORY_STATISTICS),
            cx,
        ));
        let story = session.stories.stories.get(&(chat_id, story_id));
        if let Some(story) = story {
            let title = if story.caption.is_empty() {
                tr(keys::Q_MONEY_STORY).to_string()
            } else {
                story.caption.clone()
            };
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.post_thumb(&story_thumb_candidates(story), true, cx))
                    .child(row_text(title, stamp(story.date))),
            );
        }
        let state = session
            .stories
            .insights
            .as_ref()
            .filter(|s| s.chat_id == chat_id && s.story_id == story_id);
        let public = state.map_or(0, |s| s.forwards_total);
        if let Some(info) = story.and_then(|s| s.interaction_info) {
            page = page.child(overview_grid(vec![
                OverviewCell::plain(
                    format_short(i64::from(info.view_count)),
                    tr(keys::Q_MONEY_VIEWS),
                ),
                OverviewCell::plain(
                    format_short(i64::from(public)),
                    tr(keys::Q_MONEY_PUBLIC_SHARES),
                ),
                OverviewCell::plain(
                    format_short(i64::from(info.reaction_count)),
                    tr(keys::Q_MONEY_REACTIONS),
                ),
                OverviewCell::plain(
                    format_short(i64::from(private_share_count(info.forward_count, public))),
                    tr(keys::Q_MONEY_PRIVATE_SHARES),
                ),
            ]));
        }
        let Some(state) = state else {
            return page
                .child(loading_line(tr(keys::Q_MONEY_LOADING_STATISTICS)))
                .into_any_element();
        };
        match &state.statistics {
            StoryStatsFetch::Loading => {
                page = page.child(loading_line(tr(keys::Q_MONEY_LOADING_STATISTICS)))
            }
            StoryStatsFetch::Failed(message) => {
                page = page.child(div().text_sm().text_color(danger()).child(message.clone()));
            }
            StoryStatsFetch::Loaded(stats) => {
                for (id, title, graph, kind) in [
                    (
                        "interactions",
                        tr(keys::Q_MONEY_INTERACTIONS),
                        &stats.interaction_graph,
                        ChartKind::DoubleLinear,
                    ),
                    (
                        "reactions",
                        tr(keys::Q_MONEY_REACTIONS),
                        &stats.reaction_graph,
                        ChartKind::Bar,
                    ),
                ] {
                    let id = format!("story-{chat_id}-{story_id}-{id}");
                    if let Some(section) =
                        self.stat_graph_section(chat_id, &id, title, graph, kind, cx)
                    {
                        page = page.child(divider(cx)).child(section);
                    }
                }
            }
        }
        let more = (!state.forwards_next_offset.is_empty()).then(|| {
            let offset = state.forwards_next_offset.clone();
            Button::new("story-page-forwards-more")
                .label(tr(keys::Q_MONEY_SHOW_MORE))
                .ghost()
                .small()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.fetch_story_public_forwards(
                            ChatId(chat_id),
                            story_id,
                            &offset,
                        );
                    }
                    cx.notify();
                }))
                .into_any_element()
        });
        if matches!(state.statistics, StoryStatsFetch::Loaded(_)) {
            page = page.child(super::statistics_export::export_button(chat_id, cx));
        }
        page.child(self.public_shares(
            "story-forward",
            state.forwards_total,
            &state.forwards,
            state.forwards_loading,
            state.forwards_error.clone(),
            more,
            cx,
        ))
        .into_any_element()
    }

    /// "N public shares": the channels that forwarded the post and the
    /// stories that reposted it; a forwarded message opens in its chat.
    #[allow(clippy::too_many_arguments)]
    fn public_shares(
        &self,
        id: &'static str,
        total: i32,
        forwards: &[PublicForwardView],
        loading: bool,
        error: Option<String>,
        more: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let mut list = div().flex().flex_col().gap_1();
        if total > 0 {
            list = list.child(section_title(super::statistics::plural(
                total,
                tr(keys::Q_MONEY_PUBLIC_SHARE),
                tr(keys::Q_MONEY_PUBLIC_SHARES_2),
            )));
        }
        for (index, forward) in forwards.iter().enumerate() {
            let source = match forward.kind {
                PublicForwardKind::Message { chat_id, .. }
                | PublicForwardKind::Story { chat_id, .. } => chat_id,
            };
            let chat = session.and_then(|s| s.chats.get(&source));
            let title = chat
                .map(|c| c.title.clone())
                .unwrap_or_else(|| tr_with(keys::Q_MONEY_CHAT, &[("source", &source.to_string())]));
            let photo = session
                .and_then(|s| s.chat_photo_path(ChatId(source)))
                .and_then(|path| sandboxed_display_path(path, &roots));
            let members = chat
                .and_then(|c| session.and_then(|s| s.group_member_counts(c)))
                .map(|(members, _)| members)
                .filter(|m| *m > 0);
            let is_group = chat.is_some_and(|c| {
                matches!(
                    c.kind,
                    quill::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
                )
            });
            let members_text = members.map(|m| {
                if is_group {
                    super::statistics::plural(m, "member", "members")
                } else {
                    super::statistics::plural(m, "subscriber", "subscribers")
                }
            });
            let views_text = (forward.view_count > 0)
                .then(|| super::statistics::plural(forward.view_count, "view", "views"));
            let status = match (members_text, views_text) {
                (Some(m), Some(v)) => format!("{m}, {v}"),
                (Some(text), None) | (None, Some(text)) => text,
                (None, None) => tr(keys::Q_MONEY_NO_VIEWS).to_string(),
            };
            let row = stats_row(format!("{id}-{index}").into(), cx)
                .aria_label(SharedString::from(title.clone()))
                .child(chat_avatar(&title, photo.as_deref(), 32.))
                .child(row_text(title, status));
            list = list.child(match forward.kind {
                PublicForwardKind::Message {
                    chat_id,
                    message_id,
                } => row
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.story_area_open_message(ChatId(chat_id), MessageId(message_id), cx);
                    }))
                    .into_any_element(),
                PublicForwardKind::Story { .. } => row.into_any_element(),
            });
        }
        if let Some(error) = error {
            list = list.child(div().text_sm().text_color(danger()).child(error));
        }
        if loading {
            list = list.child(loading_line(tr(keys::Q_MONEY_LOADING_PUBLIC_SHARES)));
        } else if let Some(more) = more {
            list = list.child(more);
        }
        list.into_any_element()
    }
}

fn loading_line(text: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(text_muted())
        .child(Spinner::new().small())
        .child(text)
}

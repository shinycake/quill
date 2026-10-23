//! "Export to File" on the statistics pages: the page's numbers and
//! charts as an Excel workbook (tdesktop `info_statistics_export.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{civil_local, now_unix};
use quill::stat_chart::{
    PostCounters, Sheet, StatChart, chat_statistics_sheets, export_file_name, parse_chart,
    post_statistics_sheets, serialize_workbook,
};
use quill::state::{
    ChatStatisticsFetch, GraphFetch, MessageStatsFetch, PollStatsFetch, PostLookup, Session,
    StatsPage, StoryStatsFetch, private_share_count,
};
use quill::telegram::envelope::{ChatStatistics, ChatStatisticsObject, StatisticalGraph};
use std::path::PathBuf;

/// A graph's data: a ready one, or an async one that has loaded.
fn resolve(session: &Session, chat_id: i64, graph: &StatisticalGraph) -> Option<StatChart> {
    let json = match graph {
        StatisticalGraph::Data { json_data, .. } => json_data,
        StatisticalGraph::Async { token } => match session.groups.stats.graph(chat_id, token, 0) {
            Some(GraphFetch::Loaded(StatisticalGraph::Data { json_data, .. })) => json_data,
            _ => return None,
        },
        StatisticalGraph::Error { .. } => return None,
    };
    parse_chart(json).ok()
}

/// The workbook of the page `chat_id`'s panel shows, and its file name.
pub(super) fn page_workbook(session: &Session, chat_id: i64) -> Option<(Vec<Sheet>, String)> {
    let title = session
        .chats
        .get(&chat_id)
        .map(|chat| chat.title.clone())
        .unwrap_or_default();
    let now = civil_local(now_unix());
    let today = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
    let chat_stats = match session.groups.chat_statistics.get(&chat_id) {
        Some(ChatStatisticsFetch::Loaded(stats)) => Some(stats.as_ref()),
        _ => None,
    };
    match session.groups.stats.page(chat_id) {
        StatsPage::Overview => {
            let stats = chat_stats?;
            let sheets = chat_statistics_sheets(stats, |graph| resolve(session, chat_id, graph));
            Some((sheets, export_file_name(&title, None, None, &today)))
        }
        StatsPage::Message(message_id) => {
            let state = session.groups.stats.message_for(chat_id, message_id)?;
            let MessageStatsFetch::Loaded(stats) = &state.statistics else {
                return None;
            };
            let recent = match chat_stats {
                Some(ChatStatistics::Channel(channel)) => channel
                    .recent_interactions
                    .iter()
                    .find(|info| info.object == ChatStatisticsObject::Message { message_id })
                    .copied(),
                _ => None,
            };
            let post = match session.stats_post(chat_id, message_id) {
                PostLookup::Ready(post) => Some(post),
                _ => None,
            };
            let public = state.forwards.total;
            let forwards = recent
                .map(|r| r.forward_count)
                .or(post.as_ref().map(|p| p.forwards));
            let counters = PostCounters {
                views: recent
                    .map(|r| r.view_count)
                    .or(post.as_ref().map(|p| p.views)),
                reactions: recent.map(|r| r.reaction_count),
                public_shares: Some(public),
                private_shares: forwards.map(|f| private_share_count(f, public)),
            };
            let poll = match session.messages.poll_stats.get(&(chat_id, message_id)) {
                Some(PollStatsFetch::Loaded(graph)) => resolve(session, chat_id, graph),
                _ => None,
            };
            let sheets = post_statistics_sheets(
                counters,
                &[
                    (
                        tr(keys::Q_MONEY_INTERACTIONS),
                        resolve(session, chat_id, &stats.interaction_graph),
                    ),
                    (
                        tr(keys::Q_MONEY_REACTIONS),
                        resolve(session, chat_id, &stats.reaction_graph),
                    ),
                    (tr(keys::Q_MONEY_POLL_VOTES), poll),
                ],
            );
            Some((
                sheets,
                export_file_name(&title, Some(message_id), None, &today),
            ))
        }
        StatsPage::Story(story_id) => {
            let state = session
                .stories
                .insights
                .as_ref()
                .filter(|s| s.chat_id == chat_id && s.story_id == story_id)?;
            let StoryStatsFetch::Loaded(stats) = &state.statistics else {
                return None;
            };
            let info = session
                .stories
                .stories
                .get(&(chat_id, story_id))
                .and_then(|story| story.interaction_info);
            let public = state.forwards_total;
            let counters = PostCounters {
                views: info.map(|i| i.view_count),
                reactions: info.map(|i| i.reaction_count),
                public_shares: Some(public),
                private_shares: info.map(|i| private_share_count(i.forward_count, public)),
            };
            let sheets = post_statistics_sheets(
                counters,
                &[
                    (
                        tr(keys::Q_MONEY_INTERACTIONS),
                        resolve(session, chat_id, &stats.interaction_graph),
                    ),
                    (
                        tr(keys::Q_MONEY_REACTIONS),
                        resolve(session, chat_id, &stats.reaction_graph),
                    ),
                ],
            );
            Some((
                sheets,
                export_file_name(&title, None, Some(story_id), &today),
            ))
        }
    }
}

impl QuillApp {
    /// "Export to File…": ask where, then write the workbook there.
    pub(super) fn export_statistics(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let Some((sheets, name)) = self.session().and_then(|s| page_workbook(s, chat_id)) else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_THE_STATISTICS_ARE_STILL_LOADING,
            )));
            cx.notify();
            return;
        };
        let dir = quill::media_viewer::downloads_dir().unwrap_or_else(|| PathBuf::from("."));
        let picker = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = picker.await else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move {
                    let bytes = serialize_workbook(&sheets);
                    if bytes.is_empty() {
                        return Err(());
                    }
                    std::fs::write(&dest, bytes).map(|_| dest).map_err(|_| ())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection.set_note(match written {
                    Ok(dest) => StatusNote::info(tr_with(
                        keys::STATS_EXPORT_DONE,
                        &[(
                            "file",
                            (dest
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(tr(keys::Q_MONEY_THE_FILE))),
                        )],
                    )),
                    Err(()) => StatusNote::error(tr(keys::STATS_EXPORT_ERROR)),
                });
                cx.notify();
            });
        })
        .detach();
    }
}

/// The page's "Export to file…" button.
pub(super) fn export_button(chat_id: i64, cx: &mut Context<QuillApp>) -> AnyElement {
    Button::new("statistics-export")
        .label(tr(keys::Q_MONEY_EXPORT_TO_FILE))
        .ghost()
        .small()
        .on_click(cx.listener(move |this, _, _, cx| this.export_statistics(chat_id, cx)))
        .into_any_element()
}

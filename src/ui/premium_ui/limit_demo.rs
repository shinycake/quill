#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::premium_limits::PremiumLimitKind;
use quill::state::{InfoPanelTarget, ProfileChatsFetch, ProfileChatsKind, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::CanPostStoryResult;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-premium-limits",
        seed_premium_limits_session,
        "screenshot demo — Premium limit boxes and toasts (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_premium_limits),
];

pub(super) const SIMILAR_SOURCE_CHANNEL: i64 = -1_002_000_000_901;
const SIMILAR: [(i64, &str); 3] = [
    (-1_002_000_000_911, "Design Weekly"),
    (-1_002_000_000_912, "Rust Digest"),
    (-1_002_000_000_913, "Type Foundry"),
];
const SIMILAR_TOTAL: i32 = 14;

fn channel_json(id: i64, title: &str) -> String {
    let supergroup = -id - 1_000_000_000_000;
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{supergroup},"is_channel":true}},"unread_count":0}}}}"#
    )
}

pub(super) fn seed_premium_limits_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = crate::ui::demo::seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let channels = std::iter::once((SIMILAR_SOURCE_CHANNEL, "Quill News")).chain(SIMILAR);
    for (id, title) in channels {
        if let Some(owned) = copy_and_parse(&channel_json(id, title), &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let users = &mut session.users_state;
    users.profile_chat_lists.insert(
        (ProfileChatsKind::SimilarChats, SIMILAR_SOURCE_CHANNEL),
        ProfileChatsFetch::Loaded(SIMILAR.iter().map(|(id, _)| *id).collect()),
    );
    users
        .similar_chat_totals
        .insert(SIMILAR_SOURCE_CHANNEL, SIMILAR_TOTAL);
    if std::env::var_os("QUILL_DEMO_PREMIUM_ACCOUNT").is_some() {
        session.payments.premium_option = Some(true);
    }
    session
}

impl QuillApp {
    fn demo_premium_limits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let which = std::env::var("QUILL_DEMO_PREMIUM_LIMIT").unwrap_or_default();
        let kind = match which.as_str() {
            "stickers" => PremiumLimitKind::FavoriteStickers,
            "gifs" => PremiumLimitKind::SavedGifs,
            "similar" => PremiumLimitKind::SimilarChats,
            "bots" => PremiumLimitKind::OwnedBots,
            "stories" => {
                self.open_story_composer(window, cx);
                if let Some(session) = self.demo_session.as_mut() {
                    session.stories.post.eligibility =
                        Some(CanPostStoryResult::WeeklyLimitExceeded {
                            retry_after: 2 * 86_400,
                        });
                }
                self.story_composer_after_check(cx);
                return;
            }
            "similar-unlock" => {
                self.open_info_panel_target(
                    InfoPanelTarget::Supergroup(-SIMILAR_SOURCE_CHANNEL - 1_000_000_000_000),
                    window,
                    cx,
                );
                cx.notify();
                return;
            }
            _ => PremiumLimitKind::SavedPins,
        };
        self.show_premium_limit(kind, 0, cx);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "limit_demo_tests.rs"]
mod tests;

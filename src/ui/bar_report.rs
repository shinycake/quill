//! What the chat's action bar sends (report spam, hide, unarchive, block):
//! the note says "done" when the server answered, not when the requests
//! went out, and a refusal reads as a refusal.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::connect::{ConnectDriver, JsonSender, LiveSender};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr, tr_with};
use quill::state::{Answer, RequestAcks};

/// What the bar asked for; it picks the note that ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarKind {
    Report,
    Hide,
    /// Back to the main list; the bar goes once both requests are done.
    Unarchive,
    Block {
        user_id: i64,
    },
}

/// What the "Block" box confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BlockPlan {
    pub(super) user_id: i64,
    pub(super) report: bool,
    pub(super) delete_chat: bool,
}

/// The requests the bar sent for this chat, by their ids. The chat's value
/// owns it, so another chat never shows this one's note.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) enum BarWork {
    #[default]
    Idle,
    /// The requests still unanswered (never empty); a refusal ends it at
    /// once, the last answer ends it with the done note. A second press is
    /// refused while this is out.
    Out {
        kind: BarKind,
        waiting: Vec<RequestId>,
    },
}

/// How a bar's work that was out ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarEnd {
    Done,
    Refused,
}

/// What starting the bar's work did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Started {
    Out(BarWork),
    /// A send was refused: the user reads the refusal.
    Refused,
    /// Nothing needed asking (the bar was already gone).
    Nothing,
}

impl BarWork {
    fn out<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        kind: BarKind,
        waiting: Vec<RequestId>,
    ) -> Started {
        if waiting.is_empty() {
            return Started::Nothing;
        }
        for request in &waiting {
            driver.session.acks.expect(*request);
        }
        Started::Out(Self::Out { kind, waiting })
    }

    /// The report and, for a chat that can be left (a group or a channel),
    /// the leave after it: nothing is left when the report was refused.
    pub(super) fn start_report<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        chat_id: ChatId,
        leave: bool,
    ) -> Started {
        let Ok(reported) = driver.report_chat(chat_id) else {
            return Started::Refused;
        };
        let mut waiting: Vec<RequestId> = reported.into_iter().collect();
        if leave {
            let Ok(left) = driver.leave_channel(chat_id) else {
                return Started::Refused;
            };
            waiting.push(left);
        }
        match Self::out(driver, BarKind::Report, waiting) {
            Started::Nothing => Started::Refused,
            started => started,
        }
    }

    pub(super) fn start_hide<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        chat_id: ChatId,
    ) -> Started {
        match driver.dismiss_chat_action_bar(chat_id) {
            Ok(request) => Self::out(driver, BarKind::Hide, request.into_iter().collect()),
            Err(_) => Started::Refused,
        }
    }

    pub(super) fn start_unarchive<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        chat_id: ChatId,
    ) -> Started {
        let (Ok(moved), Ok(reset)) = (
            driver.unarchive_chat(chat_id),
            driver.reset_chat_notification_settings(chat_id),
        ) else {
            return Started::Refused;
        };
        Self::out(driver, BarKind::Unarchive, vec![moved, reset])
    }

    pub(super) fn start_block<S: JsonSender>(
        driver: &mut ConnectDriver<S>,
        chat_id: ChatId,
        plan: BlockPlan,
    ) -> Started {
        let Ok(blocked) = driver.block_sender(plan.user_id) else {
            return Started::Refused;
        };
        let mut waiting = vec![blocked];
        if plan.report {
            let Ok(reported) = driver.report_chat(chat_id) else {
                return Started::Refused;
            };
            waiting.extend(reported);
        }
        if plan.delete_chat {
            let Ok(removed) = driver.remove_chat_from_list(chat_id) else {
                return Started::Refused;
            };
            waiting.extend(removed);
        }
        Self::out(
            driver,
            BarKind::Block {
                user_id: plan.user_id,
            },
            waiting,
        )
    }

    pub(super) fn is_out(&self) -> bool {
        matches!(self, Self::Out { .. })
    }

    pub(super) fn requests(&self) -> &[RequestId] {
        match self {
            Self::Idle => &[],
            Self::Out { waiting, .. } => waiting,
        }
    }

    /// Takes the answers that came. A refusal ends the work now (the rest
    /// are forgotten); the last `Done` ends it done.
    pub(super) fn settle(&mut self, acks: &mut RequestAcks) -> Option<(BarKind, BarEnd)> {
        let Self::Out { kind, waiting } = self else {
            return None;
        };
        let kind = *kind;
        let mut refused = false;
        waiting.retain(|request| match acks.take(*request) {
            Some(Answer::Done) => false,
            Some(Answer::Failed(_)) => {
                refused = true;
                false
            }
            None => true,
        });
        let end = if refused {
            Some(BarEnd::Refused)
        } else if waiting.is_empty() {
            Some(BarEnd::Done)
        } else {
            None
        };
        let end = end?;
        for request in std::mem::take(waiting) {
            acks.forget(request);
        }
        *self = Self::Idle;
        Some((kind, end))
    }
}

impl QuillApp {
    /// Starts the bar's work for the chat this window shows, and holds it
    /// in the chat's value. One at a time: a press while one is out does
    /// nothing, since its answers are what the note waits on.
    pub(super) fn start_bar_work(
        &mut self,
        chat_id: ChatId,
        kind: BarKind,
        start: impl FnOnce(&mut ConnectDriver<LiveSender>) -> Started,
        cx: &mut Context<Self>,
    ) {
        if self.chat_ui.made_for() != Some(chat_id) || self.screen.chat_ui.bar_work.is_out() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match start(&mut live.driver) {
                Started::Out(work) => self.screen.chat_ui.bar_work = work,
                Started::Refused => {
                    self.set_note(StatusNote::error(tr(refusal_key(kind))));
                }
                Started::Nothing => {}
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(chat_id.0, None);
        }
        cx.notify();
    }

    /// Each window's frame: the answers of the bar's work.
    pub(super) fn pump_bar_work(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.screen.chat_ui.bar_work = BarWork::Idle;
            return;
        };
        let Some((kind, end)) = self
            .screen
            .chat_ui
            .bar_work
            .settle(&mut live.driver.session.acks)
        else {
            return;
        };
        match (kind, end) {
            (BarKind::Report, BarEnd::Done) => {
                self.set_note(StatusNote::info(tr(keys::REPORT_SPAM_DONE)));
            }
            (BarKind::Block { user_id }, BarEnd::Done) => {
                let name = self.contact_display_name(user_id).to_string();
                self.set_note(StatusNote::silent(tr_with(
                    keys::Q_CHAT_IS_NOW_BLOCKED,
                    &[("user_id", &name)],
                )));
            }
            (BarKind::Unarchive, BarEnd::Done) => {
                let chat = self.chat_ui.made_for();
                if let (Some(live), Some(chat)) = (self.live.as_mut(), chat) {
                    let _ = live.driver.dismiss_chat_action_bar(chat);
                }
            }
            (BarKind::Hide, BarEnd::Done) => {}
            (kind, BarEnd::Refused) => {
                self.set_note(StatusNote::error(tr(refusal_key(kind))));
            }
        }
        cx.notify();
    }
}

fn refusal_key(kind: BarKind) -> quill::lang::Key {
    match kind {
        BarKind::Hide => keys::Q_CHAT_COULD_NOT_HIDE_THE_BAR,
        BarKind::Unarchive => keys::Q_CHAT_COULD_NOT_UNARCHIVE_THE_CHAT,
        BarKind::Block { .. } => keys::Q_CHAT_COULD_NOT_BLOCK_THE_USER,
        BarKind::Report => keys::Q_CHAT_COULD_NOT_REPORT_THE_CHAT,
    }
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the kit's glob brings its own `test` attribute.
    use super::{BarEnd, BarKind, BarWork, BlockPlan, Started};
    use quill::connect::{ConnectDriver, RecordingSender, prepare_connect};
    use quill::credentials::TelegramCredentials;
    use quill::diagnostics::{DiagnosticSink, MemorySink};
    use quill::ids::{AccountKey, ChatId, RequestId};
    use quill::platform::MemorySecretStore;
    use quill::state::{Answer, RequestAcks, Session};
    use quill::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    const PRIVATE: ChatId = ChatId(11);
    const GROUP: ChatId = ChatId(-12);

    fn out(ids: &[u64]) -> BarWork {
        BarWork::Out {
            kind: BarKind::Report,
            waiting: ids.iter().map(|id| RequestId(*id)).collect(),
        }
    }

    /// A ready account with a private chat and a basic group, both
    /// reportable, and a recorder of what the driver sends.
    fn driver() -> (ConnectDriver<Arc<RecordingSender>>, Arc<RecordingSender>) {
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let credentials = TelegramCredentials {
            api_id: 99,
            api_hash: "unit-test-hash-not-for-network".into(),
        };
        let dir = quill::scratch::unique_dir("quill-bar-report");
        let store = MemorySecretStore::new();
        let prepared =
            prepare_connect(&dir, AccountKey::primary(), &store, &credentials).expect("prepare");
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver = ConnectDriver::new(session, recorder.clone(), credentials, prepared);
        let seq = AtomicU64::new(0);
        let mut ingest = |json: &str| {
            if let Some(parsed) = copy_and_parse(json, &seq, &sink) {
                let _ = driver.ingest(parsed);
            }
        };
        ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        for (id, kind) in [
            (
                PRIVATE.0,
                r#"{"@type":"chatTypePrivate","user_id":11}"#.to_string(),
            ),
            (
                GROUP.0,
                r#"{"@type":"chatTypeBasicGroup","basic_group_id":12}"#.to_string(),
            ),
        ] {
            ingest(&format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"T","type":{kind},"can_be_reported":true,"can_be_deleted_only_for_self":true,"unread_count":0}}}}"#
            ));
            ingest(&format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"9","is_pinned":false}}}}"#
            ));
        }
        (driver, recorder)
    }

    /// The types of what was sent after `before` requests of setup.
    fn types(recorder: &RecordingSender, before: usize) -> Vec<String> {
        recorder
            .snapshot()
            .iter()
            .skip(before)
            .filter_map(|json| {
                let start = json.find(r#""@type":""#)? + 9;
                let rest = json.get(start..)?;
                Some(rest.get(..rest.find('"')?)?.to_string())
            })
            .collect()
    }

    /// A private chat's "Report spam" is a report alone: TDLib refuses
    /// `leaveChat` for it, so the note must not wait on one.
    #[test]
    fn a_private_chat_reports_without_leaving() {
        let (mut driver, recorder) = driver();
        let before = recorder.snapshot().len();
        let started = BarWork::start_report(&mut driver, PRIVATE, false);
        let Started::Out(BarWork::Out { waiting, .. }) = started else {
            panic!("the report is out: {started:?}");
        };
        assert_eq!(waiting.len(), 1);
        assert_eq!(types(&recorder, before), ["reportChat"]);
    }

    /// A group reports first and leaves after, and waits on both.
    #[test]
    fn a_group_reports_then_leaves() {
        let (mut driver, recorder) = driver();
        let before = recorder.snapshot().len();
        let started = BarWork::start_report(&mut driver, GROUP, true);
        let Started::Out(BarWork::Out { waiting, .. }) = started else {
            panic!("both are out: {started:?}");
        };
        assert_eq!(waiting.len(), 2);
        assert_eq!(types(&recorder, before), ["reportChat", "leaveChat"]);
    }

    /// A refused report sends no leave.
    #[test]
    fn a_refused_report_does_not_leave() {
        let (mut driver, recorder) = driver();
        let before = recorder.snapshot().len();
        recorder.refuse(Some("reportChat"));
        assert_eq!(
            BarWork::start_report(&mut driver, GROUP, true),
            Started::Refused
        );
        assert!(types(&recorder, before).is_empty(), "nothing went out");
    }

    /// Block waits on the block, the report and the removal alike.
    #[test]
    fn a_block_waits_on_every_request_it_sent() {
        let (mut driver, recorder) = driver();
        let before = recorder.snapshot().len();
        let plan = BlockPlan {
            user_id: 11,
            report: true,
            delete_chat: true,
        };
        let Started::Out(BarWork::Out { kind, waiting }) =
            BarWork::start_block(&mut driver, PRIVATE, plan)
        else {
            panic!("the block is out");
        };
        assert_eq!(kind, BarKind::Block { user_id: 11 });
        assert_eq!(waiting.len(), 3);
        assert_eq!(types(&recorder, before).len(), 3);
    }

    /// Not done when the requests go out: only when all are answered.
    #[test]
    fn the_work_is_done_only_when_all_are_answered() {
        let mut acks = RequestAcks::default();
        let mut work = out(&[1, 2]);
        acks.expect(RequestId(1));
        acks.expect(RequestId(2));
        assert_eq!(work.settle(&mut acks), None, "nothing answered");
        acks.settle(RequestId(1), Answer::Done);
        assert_eq!(work.settle(&mut acks), None, "one answered");
        acks.settle(RequestId(2), Answer::Done);
        assert_eq!(
            work.settle(&mut acks),
            Some((BarKind::Report, BarEnd::Done))
        );
        assert_eq!(work, BarWork::Idle);
    }

    /// A refusal is not "done", in either order of the answers.
    #[test]
    fn a_refusal_reads_as_refused() {
        for (first, second) in [(1, 2), (2, 1)] {
            let mut acks = RequestAcks::default();
            let mut work = out(&[1, 2]);
            acks.expect(RequestId(1));
            acks.expect(RequestId(2));
            acks.settle(RequestId(first), Answer::Failed("no".into()));
            assert_eq!(
                work.settle(&mut acks),
                Some((BarKind::Report, BarEnd::Refused))
            );
            acks.settle(RequestId(second), Answer::Done);
            assert_eq!(work.settle(&mut acks), None, "the work is over");
            assert_eq!(acks.take(RequestId(second)), None, "forgotten");
        }
    }

    /// While work is out, a second press is not started (`is_out`).
    #[test]
    fn work_that_is_out_blocks_another() {
        assert!(out(&[1]).is_out());
        assert!(!BarWork::Idle.is_out());
    }
}

//! The spam bar's "Report spam" / "Report and leave": the note says "done"
//! when the server answered, not when the requests went out.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};
use quill::state::{Answer, RequestAcks};

/// The report and the leave of this chat the bar sent, by their requests.
/// The chat's value owns it, so another chat never shows this one's note.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) enum BarReport {
    #[default]
    Idle,
    /// The requests still unanswered (never empty); a refusal ends it at
    /// once, the last answer ends it with the done note.
    Out { waiting: Vec<RequestId> },
}

/// How a report that was out ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarReportEnd {
    Done,
    Refused,
}

impl BarReport {
    /// The stage for the requests that went out: `None` when a send was
    /// refused (nothing to wait for that the user could act on) or when
    /// there is nothing to wait for.
    pub(super) fn out<E>(
        reported: &Result<Option<RequestId>, E>,
        left: &Result<RequestId, E>,
    ) -> Option<Self> {
        let (Ok(reported), Ok(left)) = (reported, left) else {
            return None;
        };
        let waiting: Vec<RequestId> = reported.iter().chain(Some(left)).copied().collect();
        Some(Self::Out { waiting })
    }

    pub(super) fn requests(&self) -> &[RequestId] {
        match self {
            Self::Idle => &[],
            Self::Out { waiting } => waiting,
        }
    }

    /// Takes the answers that came. A refusal ends the report now (the
    /// rest are forgotten); the last `Done` ends it done.
    pub(super) fn settle(&mut self, acks: &mut RequestAcks) -> Option<BarReportEnd> {
        let Self::Out { waiting } = self else {
            return None;
        };
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
            Some(BarReportEnd::Refused)
        } else if waiting.is_empty() {
            Some(BarReportEnd::Done)
        } else {
            None
        };
        if end.is_some() {
            for request in std::mem::take(waiting) {
                acks.forget(request);
            }
            *self = Self::Idle;
        }
        end
    }
}

impl QuillApp {
    pub(super) fn submit_bar_report(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let reported = live.driver.report_chat(chat_id);
            let left = live.driver.leave_channel(chat_id);
            let acks = &mut live.driver.session.acks;
            for request in self.screen.chat_ui.bar_report.requests() {
                acks.forget(*request);
            }
            match BarReport::out(&reported, &left) {
                Some(out) => {
                    for request in out.requests() {
                        acks.expect(*request);
                    }
                    self.screen.chat_ui.bar_report = out;
                }
                None => {
                    self.screen.chat_ui.bar_report = BarReport::Idle;
                    self.set_note(StatusNote::error(tr(
                        keys::Q_CHAT_COULD_NOT_REPORT_THE_CHAT,
                    )));
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_action_bar(chat_id.0, None);
        }
        cx.notify();
    }

    /// Each window's frame: the answers of the bar's report.
    pub(super) fn pump_bar_report(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.screen.chat_ui.bar_report = BarReport::Idle;
            return;
        };
        let Some(end) = self
            .screen
            .chat_ui
            .bar_report
            .settle(&mut live.driver.session.acks)
        else {
            return;
        };
        self.set_note(match end {
            BarReportEnd::Done => StatusNote::info(tr(keys::REPORT_SPAM_DONE)),
            BarReportEnd::Refused => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_REPORT_THE_CHAT)),
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the kit's glob brings its own `test` attribute.
    use super::{BarReport, BarReportEnd};
    use quill::ids::RequestId;
    use quill::state::{Answer, RequestAcks};

    fn out(ids: &[u64]) -> BarReport {
        BarReport::Out {
            waiting: ids.iter().map(|id| RequestId(*id)).collect(),
        }
    }

    /// Not done when the requests go out: only when both are answered.
    #[test]
    fn the_report_is_done_only_when_both_are_answered() {
        let mut acks = RequestAcks::default();
        let mut report = out(&[1, 2]);
        acks.expect(RequestId(1));
        acks.expect(RequestId(2));
        assert_eq!(report.settle(&mut acks), None, "nothing answered");
        acks.settle(RequestId(1), Answer::Done);
        assert_eq!(report.settle(&mut acks), None, "one answered");
        acks.settle(RequestId(2), Answer::Done);
        assert_eq!(report.settle(&mut acks), Some(BarReportEnd::Done));
        assert_eq!(report, BarReport::Idle);
    }

    /// A refused report is not "done", in either order of the answers.
    #[test]
    fn a_refused_report_reads_as_refused() {
        for (first, second) in [(1, 2), (2, 1)] {
            let mut acks = RequestAcks::default();
            let mut report = out(&[1, 2]);
            acks.expect(RequestId(1));
            acks.expect(RequestId(2));
            acks.settle(RequestId(first), Answer::Failed("no".into()));
            assert_eq!(report.settle(&mut acks), Some(BarReportEnd::Refused));
            acks.settle(RequestId(second), Answer::Done);
            assert_eq!(report.settle(&mut acks), None, "the report is over");
            assert_eq!(acks.take(RequestId(second)), None, "forgotten");
        }
    }

    /// A report that was not asked (not reportable) waits on the leave alone.
    #[test]
    fn an_unasked_report_waits_on_the_leave_only() {
        let sent = BarReport::out::<()>(&Ok(None), &Ok(RequestId(4)));
        assert_eq!(sent, Some(out(&[4])));
        assert_eq!(BarReport::out(&Ok(None), &Err(())), None);
    }
}

//! Where the chat look dialog's Apply stands: one stage per change it
//! sent, settled on that change's own answer (`ChatsState::chat_look_acks`),
//! so a failure leaves only the failed change to send again and an answer
//! of another dialog is never counted.
use super::LookChange;
use quill::ids::RequestId;
use quill::state::{Answer, RequestAcks};

/// One change Apply sent and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct LookSend {
    pub change: LookChange,
    pub stage: SendStage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) enum SendStage {
    /// Sent as this request; waits for its answer.
    InFlight(RequestId),
    Done,
    /// Not sent, or refused: the reason shows in the dialog.
    Failed(String),
}

/// The dialog's Apply stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum LookStage {
    /// Nothing sent yet.
    #[default]
    Editing,
    /// Apply was refused before sending (a boosted chat's level), with why.
    Blocked(String),
    /// Apply's changes. The dialog closes when all are `Done`; a `Failed`
    /// one shows its reason and is sent again by the next Apply.
    Sent(Vec<LookSend>),
}

impl LookStage {
    /// Some change still waits for its answer.
    pub fn applying(&self) -> bool {
        matches!(self, Self::Sent(sends) if sends.iter().any(|s| matches!(s.stage, SendStage::InFlight(_))))
    }

    /// The reason to show: the block, or the first failed change's.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Editing => None,
            Self::Blocked(why) => Some(why),
            Self::Sent(sends) => sends.iter().find_map(|s| match &s.stage {
                SendStage::Failed(why) => Some(why.as_str()),
                _ => None,
            }),
        }
    }

    /// Changes that were already acknowledged, so a new Apply does not send
    /// them again.
    pub fn done(&self) -> Vec<LookSend> {
        match self {
            Self::Sent(sends) => sends
                .iter()
                .filter(|s| s.stage == SendStage::Done)
                .cloned()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Every request that still waits, for the dialog to forget on close.
    pub fn in_flight(&self) -> Vec<RequestId> {
        match self {
            Self::Sent(sends) => sends
                .iter()
                .filter_map(|s| match s.stage {
                    SendStage::InFlight(id) => Some(id),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Settle every in-flight change whose answer came. `Some(true)` when
    /// the stage changed and everything is now `Done`, `Some(false)` when
    /// it changed otherwise, `None` when no answer came.
    pub fn settle(&mut self, acks: &mut RequestAcks) -> Option<bool> {
        let Self::Sent(sends) = self else {
            return None;
        };
        let mut changed = false;
        for send in sends.iter_mut() {
            if let SendStage::InFlight(id) = send.stage
                && let Some(answer) = acks.take(id)
            {
                send.stage = match answer {
                    Answer::Done => SendStage::Done,
                    Answer::Failed(why) => SendStage::Failed(why),
                };
                changed = true;
            }
        }
        changed.then(|| sends.iter().all(|s| s.stage == SendStage::Done))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(change: LookChange, stage: SendStage) -> LookSend {
        LookSend { change, stage }
    }

    fn acks_for(answers: &[(u64, Answer)]) -> RequestAcks {
        let mut acks = RequestAcks::default();
        for (id, answer) in answers {
            acks.expect(RequestId(*id));
            acks.settle(RequestId(*id), answer.clone());
        }
        acks
    }

    #[test]
    fn each_change_settles_on_its_own_answer() {
        let mut stage = LookStage::Sent(vec![
            sent(LookChange::Theme("a".into()), SendStage::InFlight(RequestId(1))),
            sent(LookChange::RemoveWallpaper, SendStage::InFlight(RequestId(2))),
        ]);
        let mut acks = acks_for(&[(1, Answer::Done)]);
        assert_eq!(stage.settle(&mut acks), Some(false));
        assert!(stage.applying(), "the second change still waits");
        let mut acks = acks_for(&[(2, Answer::Done)]);
        assert_eq!(stage.settle(&mut acks), Some(true));
        assert!(!stage.applying());
    }

    #[test]
    fn an_answer_nobody_asked_for_changes_nothing() {
        let mut stage = LookStage::Sent(vec![sent(
            LookChange::RemoveWallpaper,
            SendStage::InFlight(RequestId(5)),
        )]);
        let mut acks = acks_for(&[(9, Answer::Done)]);
        assert_eq!(stage.settle(&mut acks), None);
        assert!(stage.applying());
    }

    #[test]
    fn a_failed_change_waits_for_its_retry_and_keeps_the_done_ones() {
        let mut stage = LookStage::Sent(vec![
            sent(LookChange::Theme("a".into()), SendStage::InFlight(RequestId(1))),
            sent(LookChange::RemoveWallpaper, SendStage::InFlight(RequestId(2))),
        ]);
        let mut acks = acks_for(&[
            (1, Answer::Done),
            (2, Answer::Failed("BAD".into())),
        ]);
        assert_eq!(stage.settle(&mut acks), Some(false));
        assert!(!stage.applying());
        assert_eq!(stage.error(), Some("BAD"));
        assert_eq!(
            stage.done(),
            vec![sent(LookChange::Theme("a".into()), SendStage::Done)]
        );
    }
}

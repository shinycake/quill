//! Settling the delete animations with the answers to the deletes they wait
//! on: a refused delete ends its ghost, an answered one waits for the
//! message to leave the history, a ghost that ends forgets its request.
use super::*;

/// The request of the delete a ghost still holds, in either stage.
pub(super) fn held_request(ghost: &Vanishing) -> Option<RequestId> {
    match ghost.stage {
        GhostStage::Waiting {
            deletion: Deletion::Sent(request),
            ..
        }
        | GhostStage::Crumbling {
            deletion: Deletion::Sent(request),
            ..
        } => Some(request),
        _ => None,
    }
}

/// Settles the ghosts that hold a sent delete with its answer: a refused
/// one ends a waiting ghost (the message stays), an answered one waits for
/// the message to leave the history (or, crumbling already, lets go of the
/// request). A ghost that ends forgets its request.
pub(super) fn settle_ghosts(
    vanishing: &RefCell<Vec<Vanishing>>,
    now: Instant,
    acks: &mut quill::state::RequestAcks,
) {
    let Ok(mut ghosts) = vanishing.try_borrow_mut() else {
        return;
    };
    let mut requests: Vec<RequestId> = ghosts.iter().filter_map(held_request).collect();
    requests.sort_unstable_by_key(|request| request.0);
    requests.dedup();
    let answered: Vec<(RequestId, quill::state::Answer)> = requests
        .into_iter()
        .filter_map(|request| Some((request, acks.take(request)?)))
        .collect();
    ghosts.retain(|ghost| {
        let Some(request) = held_request(ghost) else {
            return true;
        };
        let refused = answered.iter().any(|(asked, answer)| {
            *asked == request && matches!(answer, quill::state::Answer::Failed(_))
        });
        if (ghost.is_waiting() && refused) || ghost.phase(now) == Phase::Done {
            acks.forget(request);
            return false;
        }
        true
    });
    for ghost in ghosts.iter_mut() {
        let (GhostStage::Waiting { deletion, .. } | GhostStage::Crumbling { deletion, .. }) =
            &mut ghost.stage;
        if let Deletion::Sent(request) = *deletion
            && answered.iter().any(|(asked, _)| *asked == request)
        {
            *deletion = Deletion::Local;
        }
    }
}

impl QuillApp {
    /// The answers to the deletes the ghosts wait on (see
    /// [`settle_ghosts`]), for the ghosts of every window: a ghost started
    /// in a chat window lives in that window's own state.
    pub(in crate::ui) fn settle_vanishing(&mut self, cx: &mut Context<Self>) {
        self.in_every_window(cx, |app, _| app.settle_vanishing_here());
    }

    fn settle_vanishing_here(&mut self) {
        let now = self.vanish_clock_now();
        let unseen = self.drop_unseen_deleted();
        let vanishing = &self.screen.history.vanishing;
        if let Some(live) = self.live.as_mut() {
            for request in unseen {
                live.driver.session.acks.forget(request);
            }
            settle_ghosts(vanishing, now, &mut live.driver.session.acks);
        }
    }

    /// Drop the waiting ghosts whose message is already deleted while
    /// their chat is not the one shown: back in that chat the message is
    /// not there to crumble, so no ghost may bring it back. The requests
    /// they held are returned to be forgotten.
    fn drop_unseen_deleted(&self) -> Vec<RequestId> {
        let shown = self.vanish_chat_shown();
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let Ok(mut vanishing) = self.history.vanishing.try_borrow_mut() else {
            return Vec::new();
        };
        let mut forgotten = Vec::new();
        vanishing.retain(|ghost| {
            let GhostStage::Waiting { deletion, .. } = ghost.stage else {
                return true;
            };
            let chat = ghost.message.chat_id;
            if Some(chat) == shown || !session.message_deleted(chat, ghost.message.id) {
                return true;
            }
            if let Deletion::Sent(request) = deletion {
                forgotten.push(request);
            }
            false
        });
        forgotten
    }
}

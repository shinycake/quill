//! Pure rules for message selection and the pin boxes, as in Telegram
//! Desktop (`boxes/pin_messages_box.cpp`, `HistoryInner` selection).

use crate::ids::MessageId;
use crate::telegram::envelope::ChatKind;

/// The extra checkbox of the pin box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinChoice {
    /// Channels, Saved Messages and old messages: none.
    None,
    /// Groups: "Notify all members", checked by default.
    NotifyAll,
    /// Private chats: "Also pin for {user}", unchecked by default.
    AlsoForPeer,
}

fn is_group(kind: &ChatKind) -> bool {
    matches!(
        kind,
        ChatKind::BasicGroup { .. }
            | ChatKind::Supergroup {
                is_channel: false,
                ..
            }
    )
}

/// Which checkbox the pin box shows. `is_self` is Saved Messages;
/// `pinning_old` is a message older than the newest pinned one.
pub fn pin_choice(kind: &ChatKind, is_self: bool, pinning_old: bool) -> PinChoice {
    match kind {
        ChatKind::Private { .. } if !is_self => PinChoice::AlsoForPeer,
        kind if is_group(kind) && !pinning_old => PinChoice::NotifyAll,
        _ => PinChoice::None,
    }
}

impl PinChoice {
    /// Whether the checkbox starts checked (`pin_messages_box.cpp`).
    pub fn default_checked(self) -> bool {
        matches!(self, Self::NotifyAll)
    }

    /// `(disable_notification, only_for_self)` for `pinChatMessage`.
    pub fn request_flags(self, checked: bool) -> (bool, bool) {
        match self {
            Self::NotifyAll => (!checked, false),
            Self::AlsoForPeer => (false, !checked),
            Self::None => (false, false),
        }
    }
}

/// The pin box question (`lng_pinned_pin_*`).
pub fn pin_question(kind: &ChatKind, pinning_old: bool) -> &'static str {
    if pinning_old {
        "Do you want to pin an older message while leaving a more recent one pinned?"
    } else if is_group(kind) {
        "Pin this message in the group?"
    } else {
        "Would you like to pin this message?"
    }
}

/// The unpin box question (`lng_pinned_unpin_sure` / `_many_sure`).
pub fn unpin_question(count: usize) -> String {
    if count > 1 {
        format!("Would you like to unpin {count} messages?")
    } else {
        "Would you like to unpin this message?".to_string()
    }
}

/// Ids between `anchor` and `target` inclusive, from the loaded `ids`
/// (ascending): Shift+click range selection. Without a loaded anchor the
/// range is just the target.
pub fn range_between(ids: &[MessageId], anchor: MessageId, target: MessageId) -> Vec<MessageId> {
    let (Some(a), Some(b)) = (
        ids.iter().position(|id| *id == anchor),
        ids.iter().position(|id| *id == target),
    ) else {
        return vec![target];
    };
    let (lo, hi) = (a.min(b), a.max(b));
    ids[lo..=hi].to_vec()
}

/// "Unpin all" question (`lng_pinned_unpin_all_sure`).
pub const UNPIN_ALL_QUESTION: &str = "Do you want to unpin all messages?";

/// Hide-the-bar question (`lng_pinned_hide_all_sure`).
pub const HIDE_PINNED_QUESTION: &str = "Do you want to hide the pinned message bar? It will stay hidden until a new message is pinned.";

/// Toggle `id` in a selection kept as a list (the scheduled-messages
/// dialog); returns whether it is selected afterwards.
pub fn toggle_id(selected: &mut Vec<MessageId>, id: MessageId) -> bool {
    if let Some(at) = selected.iter().position(|s| *s == id) {
        selected.remove(at);
        false
    } else {
        selected.push(id);
        true
    }
}

/// Drop selected ids that are not in `existing` any more (a scheduled
/// message that was sent or deleted elsewhere).
pub fn prune_selected(selected: &mut Vec<MessageId>, existing: &[MessageId]) {
    selected.retain(|id| existing.contains(id));
}

/// "Send N messages now?" (`lng_scheduled_send_now` / `_many`).
pub fn send_now_question(count: usize) -> String {
    if count == 1 {
        "Send message now?".to_string()
    } else {
        format!("Send {count} messages now?")
    }
}

/// "Do you want to delete N messages?" (`lng_selected_delete_sure*`).
pub fn delete_question(count: usize) -> String {
    match count {
        1 => "Do you want to delete this message?".to_string(),
        n => format!("Do you want to delete {n} messages?"),
    }
}

/// Most messages a selection holds (`Data::MaxSelectedItems`).
pub const MAX_SELECTED: usize = 100;

/// Ids that can still be added to a selection of `count` messages.
pub fn room_for(count: usize) -> usize {
    MAX_SELECTED.saturating_sub(count)
}

/// "Select up to this message" (`selectItemsUpTo`): the unselected ids
/// between `target` and the nearest selected message (by id), `target`
/// included, at most `room` of them. Empty without a selected message in
/// `ids`, or when `target` is already selected or unknown.
pub fn up_to(
    ids: &[MessageId],
    selected: &[MessageId],
    target: MessageId,
    room: usize,
) -> Vec<MessageId> {
    if selected.contains(&target) || !ids.contains(&target) {
        return Vec::new();
    }
    let Some(nearest) = ids
        .iter()
        .filter(|id| selected.contains(id))
        .min_by_key(|id| (id.0 - target.0).abs())
    else {
        return Vec::new();
    };
    let mut span = range_between(ids, *nearest, target);
    span.retain(|id| !selected.contains(id));
    // Closest to the selection first, so a cap keeps the contiguous part.
    if target.0 > nearest.0 {
        span.truncate(room);
    } else {
        span.reverse();
        span.truncate(room);
        span.reverse();
    }
    span
}

/// The row keyboard focus moves to from `current`: one step older
/// (`older`) or newer. With no focus (or one that left the list) it starts
/// at the newest message. Stays put at either end.
pub fn step_focus(ids: &[MessageId], current: Option<MessageId>, older: bool) -> Option<MessageId> {
    let newest = ids.last().copied()?;
    let Some(index) = current.and_then(|c| ids.iter().position(|id| *id == c)) else {
        return Some(newest);
    };
    let next = if older {
        index.saturating_sub(1)
    } else {
        (index + 1).min(ids.len() - 1)
    };
    Some(ids[next])
}

/// Shift+Up / Down: moving the focus from `old` to `new` with the range
/// anchored at `anchor`. Returns `(select, deselect)`: the ids entering
/// the anchor-to-focus range and the ids that left it.
pub fn range_delta(
    ids: &[MessageId],
    anchor: MessageId,
    old: MessageId,
    new: MessageId,
) -> (Vec<MessageId>, Vec<MessageId>) {
    let before = range_between(ids, anchor, old);
    let after = range_between(ids, anchor, new);
    let select = after
        .iter()
        .filter(|id| !before.contains(id))
        .copied()
        .collect();
    let deselect = before
        .iter()
        .filter(|id| !after.contains(id))
        .copied()
        .collect();
    (select, deselect)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SELECTED, PinChoice, pin_choice, pin_question, range_between, range_delta, room_for,
        step_focus, unpin_question, up_to,
    };
    use crate::ids::{MessageId, UserId};
    use crate::telegram::envelope::ChatKind;

    fn private() -> ChatKind {
        ChatKind::Private { user_id: UserId(5) }
    }
    fn group() -> ChatKind {
        ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: false,
        }
    }
    fn channel() -> ChatKind {
        ChatKind::Supergroup {
            supergroup_id: 2,
            is_channel: true,
        }
    }

    #[test]
    fn choice_follows_chat_kind() {
        assert_eq!(pin_choice(&private(), false, false), PinChoice::AlsoForPeer);
        assert_eq!(pin_choice(&private(), true, false), PinChoice::None);
        assert_eq!(pin_choice(&group(), false, false), PinChoice::NotifyAll);
        assert_eq!(pin_choice(&group(), false, true), PinChoice::None);
        assert_eq!(pin_choice(&channel(), false, false), PinChoice::None);
        assert_eq!(
            pin_choice(&ChatKind::BasicGroup { basic_group_id: 3 }, false, false),
            PinChoice::NotifyAll
        );
    }

    #[test]
    fn flags_map_to_the_request() {
        assert_eq!(PinChoice::NotifyAll.request_flags(true), (false, false));
        assert_eq!(PinChoice::NotifyAll.request_flags(false), (true, false));
        // Unchecked "Also pin for" keeps the pin on this side only.
        assert_eq!(PinChoice::AlsoForPeer.request_flags(false), (false, true));
        assert_eq!(PinChoice::AlsoForPeer.request_flags(true), (false, false));
        assert!(PinChoice::NotifyAll.default_checked());
        assert!(!PinChoice::AlsoForPeer.default_checked());
    }

    #[test]
    fn questions_use_tdesktop_wording() {
        assert_eq!(
            pin_question(&group(), false),
            "Pin this message in the group?"
        );
        assert_eq!(
            pin_question(&private(), false),
            "Would you like to pin this message?"
        );
        assert!(pin_question(&group(), true).contains("older message"));
        assert_eq!(unpin_question(1), "Would you like to unpin this message?");
        assert_eq!(unpin_question(3), "Would you like to unpin 3 messages?");
        assert!(super::HIDE_PINNED_QUESTION.ends_with("until a new message is pinned."));
        assert_eq!(
            super::UNPIN_ALL_QUESTION,
            "Do you want to unpin all messages?"
        );
    }

    #[test]
    fn range_is_inclusive_and_order_independent() {
        let ids: Vec<_> = [10, 20, 30, 40, 50].map(MessageId).to_vec();
        let want = vec![MessageId(20), MessageId(30), MessageId(40)];
        assert_eq!(range_between(&ids, MessageId(20), MessageId(40)), want);
        assert_eq!(range_between(&ids, MessageId(40), MessageId(20)), want);
        assert_eq!(
            range_between(&ids, MessageId(99), MessageId(30)),
            vec![MessageId(30)]
        );
    }

    fn ids() -> Vec<MessageId> {
        [10, 20, 30, 40, 50, 60].map(MessageId).to_vec()
    }

    #[test]
    fn up_to_fills_the_gap_to_the_nearest_selected() {
        let ids = ids();
        // 60 is the nearest selected message to 50.
        let got = up_to(&ids, &[MessageId(20), MessageId(60)], MessageId(50), 100);
        assert_eq!(got, vec![MessageId(50)]);
        let got = up_to(&ids, &[MessageId(20)], MessageId(50), 100);
        assert_eq!(got, vec![MessageId(30), MessageId(40), MessageId(50)]);
        // Older than the selection works the same way.
        let got = up_to(&ids, &[MessageId(50)], MessageId(20), 100);
        assert_eq!(got, vec![MessageId(20), MessageId(30), MessageId(40)]);
    }

    #[test]
    fn up_to_needs_a_selection_and_an_unselected_target() {
        let ids = ids();
        assert!(up_to(&ids, &[], MessageId(30), 100).is_empty());
        assert!(up_to(&ids, &[MessageId(30)], MessageId(30), 100).is_empty());
        assert!(up_to(&ids, &[MessageId(30)], MessageId(35), 100).is_empty());
    }

    #[test]
    fn up_to_honours_the_selection_limit() {
        let ids = ids();
        // Only two more fit: the two next to the selection, not the far end.
        let got = up_to(&ids, &[MessageId(10)], MessageId(60), 2);
        assert_eq!(got, vec![MessageId(20), MessageId(30)]);
        let got = up_to(&ids, &[MessageId(60)], MessageId(10), 2);
        assert_eq!(got, vec![MessageId(40), MessageId(50)]);
        assert_eq!(room_for(98), 2);
        assert_eq!(room_for(MAX_SELECTED + 5), 0);
    }

    #[test]
    fn list_selection_toggles_and_prunes() {
        use super::{delete_question, prune_selected, send_now_question, toggle_id};
        let mut selected = Vec::new();
        assert!(toggle_id(&mut selected, MessageId(2)));
        assert!(toggle_id(&mut selected, MessageId(5)));
        assert!(!toggle_id(&mut selected, MessageId(2)));
        assert_eq!(selected, vec![MessageId(5)]);
        prune_selected(&mut selected, &[MessageId(1), MessageId(2)]);
        assert!(selected.is_empty());
        assert_eq!(send_now_question(1), "Send message now?");
        assert_eq!(send_now_question(3), "Send 3 messages now?");
        assert_eq!(delete_question(1), "Do you want to delete this message?");
        assert_eq!(delete_question(2), "Do you want to delete 2 messages?");
    }

    #[test]
    fn focus_steps_and_clamps() {
        let ids = ids();
        assert_eq!(step_focus(&ids, None, true), Some(MessageId(60)));
        assert_eq!(step_focus(&ids, None, false), Some(MessageId(60)));
        assert_eq!(
            step_focus(&ids, Some(MessageId(40)), true),
            Some(MessageId(30))
        );
        assert_eq!(
            step_focus(&ids, Some(MessageId(40)), false),
            Some(MessageId(50))
        );
        assert_eq!(
            step_focus(&ids, Some(MessageId(10)), true),
            Some(MessageId(10))
        );
        assert_eq!(
            step_focus(&ids, Some(MessageId(60)), false),
            Some(MessageId(60))
        );
        // A focus that scrolled out of the loaded window restarts.
        assert_eq!(
            step_focus(&ids, Some(MessageId(99)), true),
            Some(MessageId(60))
        );
        assert_eq!(step_focus(&[], None, true), None);
    }

    #[test]
    fn shift_arrows_grow_and_shrink_the_range() {
        let ids = ids();
        let (add, drop) = range_delta(&ids, MessageId(30), MessageId(30), MessageId(40));
        assert_eq!((add, drop), (vec![MessageId(40)], vec![]));
        let (add, drop) = range_delta(&ids, MessageId(30), MessageId(50), MessageId(40));
        assert_eq!((add, drop), (vec![], vec![MessageId(50)]));
        // Crossing the anchor flips the side.
        let (add, drop) = range_delta(&ids, MessageId(30), MessageId(40), MessageId(20));
        assert_eq!(add, vec![MessageId(20)]);
        assert_eq!(drop, vec![MessageId(40)]);
    }
}

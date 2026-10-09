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

#[cfg(test)]
mod tests {
    use super::{PinChoice, pin_choice, pin_question, range_between, unpin_question};
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
}

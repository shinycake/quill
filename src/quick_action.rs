//! The double-click-on-a-message action (tdesktop
//! `HistoryView::DoubleClickQuickAction`, set in Settings > Chat): reply
//! (the default), react with the quick reaction, or nothing. Kept pure so
//! the dispatch is unit tested.

use serde::{Deserialize, Serialize};

/// What a double-click on a message does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuickAction {
    /// Start a reply to the message (tdesktop default).
    #[default]
    Reply,
    /// Toggle the quick reaction on the message.
    React,
    /// Do nothing.
    None,
}

/// The effect a double-click has once the setting meets the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickEffect {
    Reply,
    React,
    Nothing,
}

impl QuickAction {
    /// Radio order in settings. tdesktop shows Reply and React; `None` is a
    /// stored value there that has no radio, Quill lists it too.
    pub const ALL: [(QuickAction, &'static str); 3] = [
        (QuickAction::Reply, "Reply"),
        (QuickAction::React, "React with the quick reaction"),
        (QuickAction::None, "Do nothing"),
    ];

    /// Index of `self` in [`Self::ALL`].
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|(action, _)| *action == self)
            .unwrap_or(0)
    }

    /// Resolve the effect. `sent` is false for pending or local messages,
    /// which can be neither replied to nor reacted to.
    pub fn effect(self, sent: bool) -> QuickEffect {
        match (self, sent) {
            (_, false) | (QuickAction::None, _) => QuickEffect::Nothing,
            (QuickAction::Reply, true) => QuickEffect::Reply,
            (QuickAction::React, true) => QuickEffect::React,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{QuickAction, QuickEffect};

    #[test]
    fn default_is_reply_like_tdesktop() {
        assert_eq!(QuickAction::default(), QuickAction::Reply);
    }

    #[test]
    fn effect_dispatch() {
        assert_eq!(QuickAction::Reply.effect(true), QuickEffect::Reply);
        assert_eq!(QuickAction::React.effect(true), QuickEffect::React);
        assert_eq!(QuickAction::None.effect(true), QuickEffect::Nothing);
        for action in [QuickAction::Reply, QuickAction::React, QuickAction::None] {
            assert_eq!(action.effect(false), QuickEffect::Nothing);
        }
    }

    #[test]
    fn index_matches_all() {
        for (ix, (action, _)) in QuickAction::ALL.iter().enumerate() {
            assert_eq!(action.index(), ix);
        }
    }

    #[test]
    fn serde_round_trip() {
        for (action, _) in QuickAction::ALL {
            let json = serde_json::to_string(&action).unwrap();
            assert_eq!(serde_json::from_str::<QuickAction>(&json).unwrap(), action);
        }
        assert_eq!(
            serde_json::from_str::<QuickAction>("\"react\"").unwrap(),
            QuickAction::React
        );
    }
}

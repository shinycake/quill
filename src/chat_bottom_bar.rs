//! Which full-width action replaces the composer, mirroring Telegram
//! Desktop's `HistoryWidget::updateControlsVisibility` (`isBlocked`,
//! `isJoinChannel`, `isMuteUnmute`, `isBotStart`, in that order).
#![deny(clippy::indexing_slicing)]

use crate::ids::RequestId;
use crate::telegram::envelope::ChannelMemberStatus;

/// The action bar shown in place of the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomBar {
    /// The peer is blocked. Bots read "Restart" (`lng_restart_button`),
    /// everyone else "Unblock".
    Unblock { restart: bool },
    /// A channel the viewer has not joined.
    JoinChannel,
    /// A supergroup the viewer has not joined.
    JoinGroup,
    /// A supergroup that approves new members (`requestToJoin`).
    ApplyToJoin,
    /// A channel the viewer can read but not post in.
    MuteUnmute { muted: bool },
    /// A bot chat that has not been started.
    Start,
    /// The same, with the start request out: the button is off.
    Starting,
    /// The user accepts messages only from contacts and Premium users,
    /// and the viewer is neither (`EmptyChatLockedBox::PremiumRequired`).
    PremiumRequired,
}

/// Whether a bot chat offers its Start button: one value, read from the
/// chat each frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StartState {
    /// The chat is not a bot's, or it has been started.
    #[default]
    NotStartable,
    /// A start link armed a parameter, or the history loaded empty.
    Offered,
    /// `sendBotStartMessage` is out as this request: the button waits for
    /// its answer, a refusal offers Start again.
    Starting(RequestId),
}

/// What the bar decision reads from the open chat.
#[derive(Debug, Clone, Copy, Default)]
pub struct BarFacts {
    /// The peer of a private or secret chat is blocked.
    pub blocked: bool,
    pub is_bot: bool,
    /// The viewer may send messages here at all.
    pub can_send: bool,
    pub start: StartState,
    /// A supergroup or channel (`None` for private chats and basic groups).
    pub channel: Option<ChannelFacts>,
    pub muted: bool,
    /// The paid-messages gate says Premium is needed to write.
    pub premium_required: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ChannelFacts {
    pub broadcast: bool,
    /// `None` until membership is known.
    pub membership: Option<ChannelMemberStatus>,
    pub join_by_request: bool,
    /// A channel's direct messages group: anyone may write without
    /// joining (`supergroup.is_direct_messages_group`).
    pub direct_messages: bool,
}

/// `None` keeps the composer (or the existing note) as is.
pub fn bottom_bar(facts: &BarFacts) -> Option<BottomBar> {
    if facts.blocked {
        return Some(BottomBar::Unblock {
            restart: facts.is_bot,
        });
    }
    if facts.premium_required {
        return Some(BottomBar::PremiumRequired);
    }
    if let Some(channel) = facts.channel.filter(|channel| !channel.direct_messages) {
        match channel.membership {
            Some(ChannelMemberStatus::Left) => {
                return Some(if channel.broadcast {
                    BottomBar::JoinChannel
                } else if channel.join_by_request {
                    BottomBar::ApplyToJoin
                } else {
                    BottomBar::JoinGroup
                });
            }
            Some(ChannelMemberStatus::Member) if channel.broadcast => {
                return Some(BottomBar::MuteUnmute { muted: facts.muted });
            }
            _ => {}
        }
    }
    if !(facts.is_bot && facts.can_send) {
        return None;
    }
    match facts.start {
        StartState::NotStartable => None,
        StartState::Offered => Some(BottomBar::Start),
        StartState::Starting(_) => Some(BottomBar::Starting),
    }
}

impl BottomBar {
    /// Button text. Sentence case, as the rest of Quill's copy.
    pub fn label(self) -> &'static str {
        match self {
            BottomBar::Unblock { restart: true } => "Restart",
            BottomBar::Unblock { restart: false } => "Unblock",
            BottomBar::JoinChannel => "Join channel",
            BottomBar::JoinGroup => "Join group",
            BottomBar::ApplyToJoin => "Apply to join group",
            BottomBar::MuteUnmute { muted: true } => "Unmute",
            BottomBar::MuteUnmute { muted: false } => "Mute",
            BottomBar::Start | BottomBar::Starting => "Start",
            BottomBar::PremiumRequired => "Go Premium",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BarFacts, BottomBar, ChannelFacts, StartState, bottom_bar};
    use crate::telegram::envelope::ChannelMemberStatus as Status;

    fn channel(broadcast: bool, membership: Option<Status>, by_request: bool) -> BarFacts {
        BarFacts {
            can_send: true,
            channel: Some(ChannelFacts {
                broadcast,
                membership,
                join_by_request: by_request,
                direct_messages: false,
            }),
            ..BarFacts::default()
        }
    }

    #[test]
    fn premium_gate_replaces_the_composer_after_the_block() {
        let gated = BarFacts {
            can_send: true,
            premium_required: true,
            ..BarFacts::default()
        };
        assert_eq!(bottom_bar(&gated), Some(BottomBar::PremiumRequired));
        assert_eq!(BottomBar::PremiumRequired.label(), "Go Premium");
        let blocked = BarFacts {
            blocked: true,
            ..gated
        };
        assert_eq!(
            bottom_bar(&blocked),
            Some(BottomBar::Unblock { restart: false })
        );
    }

    #[test]
    fn plain_chat_keeps_the_composer() {
        assert_eq!(
            bottom_bar(&BarFacts {
                can_send: true,
                ..BarFacts::default()
            }),
            None
        );
    }

    #[test]
    fn blocked_peer_wins_and_bots_restart() {
        let blocked = BarFacts {
            blocked: true,
            ..BarFacts::default()
        };
        assert_eq!(
            bottom_bar(&blocked),
            Some(BottomBar::Unblock { restart: false })
        );
        let bot = BarFacts {
            is_bot: true,
            start: StartState::Offered,
            can_send: true,
            ..blocked
        };
        assert_eq!(bottom_bar(&bot), Some(BottomBar::Unblock { restart: true }));
        assert_eq!(BottomBar::Unblock { restart: true }.label(), "Restart");
        assert_eq!(BottomBar::Unblock { restart: false }.label(), "Unblock");
    }

    #[test]
    fn leaving_a_channel_or_group_offers_to_join() {
        assert_eq!(
            bottom_bar(&channel(true, Some(Status::Left), false)),
            Some(BottomBar::JoinChannel)
        );
        assert_eq!(
            bottom_bar(&channel(false, Some(Status::Left), false)),
            Some(BottomBar::JoinGroup)
        );
        // A channel never asks to apply, whatever the flag says.
        assert_eq!(
            bottom_bar(&channel(true, Some(Status::Left), true)),
            Some(BottomBar::JoinChannel)
        );
        assert_eq!(
            bottom_bar(&channel(false, Some(Status::Left), true)),
            Some(BottomBar::ApplyToJoin)
        );
        assert_eq!(BottomBar::ApplyToJoin.label(), "Apply to join group");
    }

    #[test]
    fn direct_messages_keep_the_composer_without_joining() {
        let mut facts = channel(false, Some(Status::Left), false);
        if let Some(channel) = facts.channel.as_mut() {
            channel.direct_messages = true;
        }
        assert_eq!(bottom_bar(&facts), None);
    }

    #[test]
    fn unknown_membership_decides_nothing() {
        assert_eq!(bottom_bar(&channel(false, None, false)), None);
        assert_eq!(bottom_bar(&channel(true, None, false)), None);
        assert_eq!(
            bottom_bar(&channel(false, Some(Status::Member), false)),
            None
        );
    }

    #[test]
    fn channel_members_mute_and_unmute() {
        let mut facts = channel(true, Some(Status::Member), false);
        assert_eq!(
            bottom_bar(&facts),
            Some(BottomBar::MuteUnmute { muted: false })
        );
        facts.muted = true;
        let bar = bottom_bar(&facts);
        assert_eq!(bar, Some(BottomBar::MuteUnmute { muted: true }));
        assert_eq!(bar.map(BottomBar::label), Some("Unmute"));
        // Admins and the creator keep the composer.
        assert_eq!(
            bottom_bar(&channel(true, Some(Status::Creator), false)),
            None
        );
        assert_eq!(
            bottom_bar(&channel(true, Some(Status::Administrator), false)),
            None
        );
    }

    #[test]
    fn unstarted_bots_start() {
        let bot = BarFacts {
            is_bot: true,
            can_send: true,
            start: StartState::Offered,
            ..BarFacts::default()
        };
        assert_eq!(bottom_bar(&bot), Some(BottomBar::Start));
        assert_eq!(
            bottom_bar(&BarFacts {
                start: StartState::NotStartable,
                ..bot
            }),
            None
        );
        // The request out keeps the button, switched off.
        assert_eq!(
            bottom_bar(&BarFacts {
                start: StartState::Starting(crate::ids::RequestId(4)),
                ..bot
            }),
            Some(BottomBar::Starting)
        );
        // A chat the viewer cannot write in has nothing to start.
        assert_eq!(
            bottom_bar(&BarFacts {
                can_send: false,
                ..bot
            }),
            None
        );
    }

    #[test]
    fn join_comes_before_start() {
        let mut facts = channel(false, Some(Status::Left), false);
        facts.is_bot = true;
        facts.start = StartState::Offered;
        assert_eq!(bottom_bar(&facts), Some(BottomBar::JoinGroup));
    }
}

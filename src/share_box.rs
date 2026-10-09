//! Share box and forward bar logic (tdesktop `ShareBox`,
//! `HistoryView::ForwardPanel`), kept pure so it is unit tested.

use crate::composer::{ComposerScheduling, SendOptions};
use crate::ids::ChatId;

/// How the share box sends: its Send button, the "Send without sound" menu
/// entry and the schedule picker (`SendMenu::Action` in tdesktop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShareSend {
    #[default]
    Normal,
    Silent,
    Scheduled(ComposerScheduling),
}

impl ShareSend {
    /// The `messageSendOptions` the choice maps to.
    pub fn options(self) -> SendOptions {
        match self {
            ShareSend::Normal => SendOptions::default(),
            ShareSend::Silent => SendOptions {
                disable_notification: true,
                ..SendOptions::default()
            },
            ShareSend::Scheduled(scheduling) => SendOptions {
                scheduling,
                ..SendOptions::default()
            },
        }
    }
}

/// Destinations ticked in the share box, in tick order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareSelection {
    chats: Vec<ChatId>,
}

impl ShareSelection {
    /// Tick or untick `chat`; returns whether it is now ticked.
    pub fn toggle(&mut self, chat: ChatId) -> bool {
        if let Some(index) = self.chats.iter().position(|id| *id == chat) {
            self.chats.remove(index);
            false
        } else {
            self.chats.push(chat);
            true
        }
    }

    pub fn contains(&self, chat: ChatId) -> bool {
        self.chats.contains(&chat)
    }

    pub fn chats(&self) -> &[ChatId] {
        &self.chats
    }

    pub fn len(&self) -> usize {
        self.chats.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chats.is_empty()
    }

    pub fn clear(&mut self) {
        self.chats.clear();
    }

    /// The only ticked chat (the forward bar can follow it).
    pub fn single(&self) -> Option<ChatId> {
        match self.chats.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }
}

/// The forward bar's two lines: who the messages are from and a preview.
/// tdesktop `ForwardPanel::checkTexts`: hidden sender names say "Sender names
/// removed", two names join with "and", more show the first and a count;
/// one message previews its text, several only count.
pub fn forward_bar_lines(
    senders: &[String],
    count: usize,
    hide_sender: bool,
    preview: &str,
) -> (String, String) {
    let from = if hide_sender || senders.is_empty() {
        "Sender names removed".to_string()
    } else if senders.len() == 1 {
        senders[0].clone()
    } else if senders.len() == 2 {
        format!("{} and {}", senders[0], senders[1])
    } else if senders.len() == 3 {
        format!("{} and 2 others", senders[0])
    } else {
        format!("{} and {} others", senders[0], senders.len() - 1)
    };
    let text = if count < 2 {
        preview.to_string()
    } else {
        format!("{count} forwarded messages")
    };
    (from, text)
}

/// Confirmation after a share box send (tdesktop `lng_share_message_to_*`).
pub fn share_done_label(messages: usize, destinations: &[String]) -> String {
    let noun = if messages == 1 { "Message" } else { "Messages" };
    match destinations {
        [] => format!("{noun} not sent"),
        [one] => format!("{noun} forwarded to {one}."),
        [a, b] => format!("{noun} forwarded to {a} and {b}."),
        many => format!("{noun} forwarded to {} chats.", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_toggles_in_tick_order() {
        let mut selection = ShareSelection::default();
        assert!(selection.toggle(ChatId(5)));
        assert!(selection.toggle(ChatId(2)));
        assert_eq!(selection.chats(), &[ChatId(5), ChatId(2)]);
        assert_eq!(selection.single(), None);
        assert!(!selection.toggle(ChatId(5)));
        assert_eq!(selection.single(), Some(ChatId(2)));
        assert!(selection.contains(ChatId(2)));
        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn send_modes_map_to_options() {
        assert_eq!(ShareSend::Normal.options(), SendOptions::default());
        assert!(ShareSend::Silent.options().disable_notification);
        let scheduled = ShareSend::Scheduled(ComposerScheduling::SendAtDate(9)).options();
        assert_eq!(scheduled.scheduling, ComposerScheduling::SendAtDate(9));
        assert!(!scheduled.disable_notification);
    }

    #[test]
    fn forward_bar_wording_follows_tdesktop() {
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            forward_bar_lines(&names(&["Ada"]), 1, false, "Hi"),
            ("Ada".into(), "Hi".into())
        );
        assert_eq!(
            forward_bar_lines(&names(&["Ada", "Bob"]), 2, false, "Hi"),
            ("Ada and Bob".into(), "2 forwarded messages".into())
        );
        assert_eq!(
            forward_bar_lines(&names(&["Ada", "Bob", "Cy"]), 3, false, "").0,
            "Ada and 2 others"
        );
        assert_eq!(
            forward_bar_lines(&names(&["Ada", "Bob", "Cy", "Di"]), 4, false, "").0,
            "Ada and 3 others"
        );
        assert_eq!(
            forward_bar_lines(&names(&["Ada"]), 1, true, "Hi").0,
            "Sender names removed"
        );
    }

    #[test]
    fn done_label_names_one_or_two_chats_then_counts() {
        let n = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            share_done_label(1, &n(&["Ada"])),
            "Message forwarded to Ada."
        );
        assert_eq!(
            share_done_label(2, &n(&["Ada", "Bob"])),
            "Messages forwarded to Ada and Bob."
        );
        assert_eq!(
            share_done_label(1, &n(&["A", "B", "C"])),
            "Message forwarded to 3 chats."
        );
    }
}

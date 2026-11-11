//! The `boostChat` chain: `getAvailableChatBoostSlots`, then `boostChat`
//! with the slot the answer offers (`ConnectDriver::maybe_continue_boost`).
use crate::telegram::envelope::ParsedBoostSlot;

/// Where a user's boost press stands, one value instead of an intent and a
/// stash of slots kept in step by hand.
///
/// `WaitingSlots` leaves on the slots answer (`SlotsReady`), the slots
/// error (`Idle`), or the box closing (`Idle`, [`BoostChain::cancel`]).
/// `SlotsReady` leaves when the driver's chain consumes it
/// ([`BoostChain::take_ready`]), or when the box closes. A slots answer
/// that finds no chain waiting for its chat is dropped, so a closed box
/// leaves nothing for the next press.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum BoostChain {
    #[default]
    Idle,
    /// `getAvailableChatBoostSlots` was asked for `chat_id`.
    WaitingSlots { chat_id: i64 },
    /// The slots of `chat_id` came; the driver continues with them.
    SlotsReady {
        chat_id: i64,
        slots: Vec<ParsedBoostSlot>,
    },
}

impl BoostChain {
    /// Whether a boost of `chat_id` waits on its slots or on the chain
    /// taking them.
    pub fn is_for(&self, chat_id: i64) -> bool {
        match self {
            Self::Idle => false,
            Self::WaitingSlots { chat_id: id } | Self::SlotsReady { chat_id: id, .. } => {
                *id == chat_id
            }
        }
    }

    /// A press for `chat_id` starts (or takes over) the chain.
    pub fn wait_for(&mut self, chat_id: i64) {
        *self = Self::WaitingSlots { chat_id };
    }

    /// The slots answer of `chat_id`: kept only while a boost of that chat
    /// waits for it.
    pub fn slots_arrived(&mut self, chat_id: i64, slots: Vec<ParsedBoostSlot>) {
        if self.is_for(chat_id) {
            *self = Self::SlotsReady { chat_id, slots };
        }
    }

    /// The chain's slots, once they came. The chain is over.
    pub fn take_ready(&mut self) -> Option<(i64, Vec<ParsedBoostSlot>)> {
        match std::mem::take(self) {
            Self::SlotsReady { chat_id, slots } => Some((chat_id, slots)),
            other => {
                *self = other;
                None
            }
        }
    }

    /// The slots request of `chat_id` failed: its chain cannot continue.
    pub fn slots_failed(&mut self, chat_id: i64) {
        if matches!(self, Self::WaitingSlots { chat_id: id } if *id == chat_id) {
            *self = Self::Idle;
        }
    }

    /// The box closed: no boost waits any more.
    pub fn cancel(&mut self) {
        *self = Self::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: i32) -> ParsedBoostSlot {
        ParsedBoostSlot {
            slot_id: id,
            chat_id: 0,
            start_date: 0,
            expiration_date: 0,
            cooldown_until_date: 0,
        }
    }

    #[test]
    fn slots_are_kept_only_for_the_chat_that_waits() {
        let mut chain = BoostChain::Idle;
        chain.slots_arrived(13, vec![slot(1)]);
        assert_eq!(chain, BoostChain::Idle, "nothing waited");
        chain.wait_for(13);
        chain.slots_arrived(14, vec![slot(2)]);
        assert_eq!(chain, BoostChain::WaitingSlots { chat_id: 13 });
        chain.slots_arrived(13, vec![slot(3)]);
        let (chat, slots) = chain.take_ready().expect("ready");
        assert_eq!((chat, slots.len()), (13, 1));
        assert_eq!(chain, BoostChain::Idle);
    }

    #[test]
    fn a_failed_request_or_a_closed_box_ends_the_chain() {
        let mut chain = BoostChain::Idle;
        chain.wait_for(13);
        chain.slots_failed(14);
        assert!(chain.is_for(13), "another chat's failure leaves it");
        chain.slots_failed(13);
        assert_eq!(chain, BoostChain::Idle);
        chain.wait_for(13);
        chain.slots_arrived(13, vec![slot(1)]);
        chain.cancel();
        assert!(chain.take_ready().is_none());
    }

    #[test]
    fn a_new_press_replaces_a_chain_for_another_chat() {
        let mut chain = BoostChain::Idle;
        chain.wait_for(13);
        chain.slots_arrived(13, vec![slot(1)]);
        chain.wait_for(14);
        assert_eq!(chain, BoostChain::WaitingSlots { chat_id: 14 });
    }
}

//! What the engine needs for a conference call: an end-to-end encrypted
//! group call that isn't bound to a chat (tdesktop `GroupCall::conference()`).
//! Every frame goes through [`FrameCrypto`] (tgcalls' `e2eEncryptDecrypt`),
//! and decrypting needs the sender of each audio ssrc ([`SsrcOwners`]).

/// A conference call's frame encryption, asked synchronously on the
/// engine's media threads for every audio and video frame. `None` drops
/// the frame, as tdesktop drops one tde2e refuses
/// (`TdE2E::EncryptDecrypt::callback`).
pub trait FrameCrypto: Send + Sync {
    /// An outgoing frame; the first `unencrypted_prefix` bytes stay in the
    /// clear.
    fn encrypt(&self, frame: &[u8], unencrypted_prefix: i32) -> Option<Vec<u8>>;

    /// An incoming frame from the participant `sender_user_id`.
    fn decrypt(&self, sender_user_id: i64, frame: &[u8]) -> Option<Vec<u8>>;
}

/// Who sends each audio ssrc in a conference call (tgcalls'
/// `MediaChannelDescription`), from the participants TDLib reports
/// (`audio_source_id`, `screen_sharing_audio_source_id`). tdesktop answers
/// tgcalls' `requestMediaChannelDescriptions` from its participant list
/// and loads the participants it doesn't know yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SsrcOwners {
    /// `(ssrc, user id)`, one per known source.
    pub owners: Vec<(u32, i64)>,
    /// Every participant is loaded: an ssrc missing from `owners` has
    /// no owner to wait for (tdesktop's resolved but unknown ssrc).
    pub complete: bool,
}

impl SsrcOwners {
    pub fn owner(&self, ssrc: u32) -> Option<i64> {
        self.owners
            .iter()
            .find(|(source, _)| *source == ssrc)
            .map(|(_, user_id)| *user_id)
    }
}

/// The participants' audio sources as [`SsrcOwners`]: users only (a
/// conference call has no channels), a zero source skipped.
pub fn conference_ssrc_owners(
    participants: &[crate::telegram::envelope::ParsedGroupCallParticipant],
    complete: bool,
) -> SsrcOwners {
    use crate::telegram::envelope::MessageSender;
    let mut owners = Vec::new();
    for participant in participants {
        let MessageSender::User { user_id } = participant.participant_id else {
            continue;
        };
        for source in [
            participant.audio_source_id,
            participant.screen_sharing_audio_source_id,
        ] {
            // TDLib sends the ssrc as a signed 32-bit number.
            let ssrc = source as u32;
            if ssrc != 0 && !owners.iter().any(|(known, _)| *known == ssrc) {
                owners.push((ssrc, user_id));
            }
        }
    }
    SsrcOwners { owners, complete }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{
        MessageSender, ParsedGroupCallParticipant, parse_group_call_participant,
    };
    use serde_json::json;

    fn participant(sender: MessageSender, audio: i32, screen: i32) -> ParsedGroupCallParticipant {
        let sender = match sender {
            MessageSender::User { user_id } => {
                json!({"@type": "messageSenderUser", "user_id": user_id})
            }
            MessageSender::Chat { chat_id } => {
                json!({"@type": "messageSenderChat", "chat_id": chat_id})
            }
        };
        let value = json!({
            "participant_id": sender,
            "audio_source_id": audio,
            "screen_sharing_audio_source_id": screen,
            "order": "1",
        });
        parse_group_call_participant(Some(&value)).expect("participant")
    }

    #[test]
    fn owners_come_from_both_audio_sources_of_users() {
        let owners = conference_ssrc_owners(
            &[
                participant(MessageSender::User { user_id: 7 }, 100, -5),
                participant(MessageSender::User { user_id: 8 }, 0, 0),
                participant(MessageSender::Chat { chat_id: -9 }, 300, 0),
            ],
            true,
        );
        assert_eq!(owners.owners, vec![(100, 7), (u32::MAX - 4, 7)]);
        assert!(owners.complete);
        assert_eq!(owners.owner(100), Some(7));
        assert_eq!(owners.owner(300), None);
    }
}

//! Connect driver: message effects (`getMessageEffect`).
use super::*;
use crate::ids::RequestId;
use crate::message_effects::get_message_effect;
use crate::state::{RequestPurpose, StickersPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask for the effects among `ids` the session does not know yet (the
    /// ones on messages in view, or the picker's list). Each id is asked
    /// for once; a 404 marks it as gone.
    pub fn load_message_effects(
        &mut self,
        ids: impl IntoIterator<Item = i64>,
    ) -> Result<Vec<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // An ask whose request is gone (a reconnect dropped it) is open again.
        let pending: std::collections::HashSet<i64> = self
            .session
            .requests
            .pending
            .values()
            .filter_map(|pending| match pending.purpose {
                RequestPurpose::Stickers(StickersPurpose::GetMessageEffect { effect_id }) => {
                    Some(effect_id)
                }
                _ => None,
            })
            .collect();
        self.session.stickers.effects.forget_dropped(&pending);
        let now = std::time::Instant::now();
        let wanted = self.session.stickers.effects.wanted(ids, now);
        let mut sent = Vec::new();
        for effect_id in wanted {
            let extra = self.session.request(
                RequestPurpose::Stickers(StickersPurpose::GetMessageEffect { effect_id }),
                None,
            );
            let json = get_message_effect(extra, effect_id);
            match self.send_json_request(extra, &json) {
                Ok(request) => {
                    self.session.stickers.effects.mark_requested(effect_id);
                    sent.push(request);
                }
                Err(err) => {
                    self.session.stickers.effects.fail_send(effect_id, now);
                    return Err(err);
                }
            }
        }
        Ok(sent)
    }

    /// The send menu's effect picker opened: describe every effect the
    /// server offers (tdesktop has them all from `messages.availableEffects`)
    /// and fetch the static icons of the ones known, which the "Send with
    /// Effect" card's message footer shows (`preloadEffectImageFor`). The
    /// pickers' animations download with their cells.
    pub fn load_effect_picker(&mut self) -> Result<Vec<RequestId>, ConnectSendError> {
        let ids: Vec<i64> = self.session.stickers.effects.available_ids().collect();
        let mut sent = self.load_message_effects(ids)?;
        let icons: Vec<crate::ids::FileId> = self
            .session
            .stickers
            .effects
            .available_ids()
            .filter_map(|id| self.session.stickers.effects.get(id))
            .filter_map(|effect| effect.icon_file())
            .collect();
        for file in icons {
            if let Some(extra) = self.download_file(file, EFFECT_ICON_PRIORITY)? {
                sent.push(extra);
            }
        }
        Ok(sent)
    }
}

/// The effect icons are small and wanted on screen now.
const EFFECT_ICON_PRIORITY: i32 = 1;

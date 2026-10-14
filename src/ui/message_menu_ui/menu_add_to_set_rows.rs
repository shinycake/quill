//! The "Add to Sticker Set" / "Add to Emoji Set" page of the message menu:
//! one row per set the user created (tdesktop `AddAddToOwnedSetAction`).
use super::*;

impl QuillApp {
    pub(in crate::ui) fn menu_add_to_set_rows(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: bool,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let message = session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0));
        let Some(MessageContent::Sticker(sticker)) = message.map(|m| {
            quill::telegram::envelope::effective_content(&m.content, m.ephemeral.as_ref())
        }) else {
            return Vec::new();
        };
        let item = quill::telegram::envelope::StickerItem {
            custom_emoji_id: None,
            id: 0,
            set_id: sticker.set_id,
            emoji: sticker.emoji.clone(),
            width: sticker.width,
            height: sticker.height,
            format: sticker.format,
            file_id: sticker.file_id,
            thumb_file_id: sticker.thumb_file_id,
            thumb_width: sticker.thumb_width,
            thumb_height: sticker.thumb_height,
            requires_premium: sticker.requires_premium,
            premium_animation: sticker.premium_animation,
        };
        let kind = quill::sticker_set_box::SetKind::of(emoji);
        let mut rows = Vec::new();
        for (index, (set_id, title)) in session.owned_sticker_sets(emoji).into_iter().enumerate() {
            let item = item.clone();
            let order = u8::try_from(index + 1).unwrap_or(u8::MAX);
            rows.push(menu_row(
                order,
                IconName::Sticker,
                // The set id keeps the row's identity stable (one row per
                // owned set, however many there are). The cast only reads
                // the id's bits; it cannot fail.
                ("menu-add-to-set", set_id as u64),
                title,
                false,
                cx,
                move |this, _, cx| {
                    this.add_existing_to_owned_set(set_id, kind, &item, cx);
                },
            ));
        }
        rows
    }
}

//! "Get Boosts via Gifts" and the prepaid giveaways on the boosts page
//! (tdesktop `FillGetBoostsButton` and the "Prepaid giveaways" list in
//! `info/channel_statistics/boosts/info_boosts_inner_widget.cpp`).

use super::app::QuillApp;
use super::giveaway_create_ui::prepaid_summary;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// The button that opens the giveaway form, then the giveaways already
    /// paid for. Channels and groups only.
    pub(super) fn giveaway_boosts_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return div().into_any_element();
        };
        if !matches!(chat.kind, ChatKind::Supergroup { .. }) {
            return div().into_any_element();
        }
        let place = if chat.kind.is_channel() {
            ("channel", "subscribers")
        } else {
            ("group", "members")
        };
        let prepaid = session
            .groups
            .chat_boost_details
            .get(&chat_id.0)
            .map(|status| status.prepaid_giveaways.clone())
            .unwrap_or_default();
        let multiplier = quill::giveaway::BOOSTS_PER_PREMIUM;
        let mut section = div().flex().flex_col().gap_1().pt_2().child(
            Button::new("boosts-get-via-gifts")
                .label("Get Boosts via Gifts")
                .small()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_giveaway_form(chat_id, None, window, cx);
                })),
        );
        section = section.child(div().text_xs().text_color(muted).child(format!(
            "Get more boosts for your {} by gifting Telegram Premium to your {}.",
            place.0, place.1
        )));
        if !prepaid.is_empty() {
            section = section
                .child(
                    div()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child("Prepaid giveaways"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Select a giveaway you already paid for to set it up."),
                );
            for (index, giveaway) in prepaid.into_iter().enumerate() {
                let boosts = if giveaway.boost_count > 0 {
                    giveaway.boost_count
                } else {
                    giveaway.winner_count.saturating_mul(multiplier)
                };
                let label = format!(
                    "{} · {} boosts",
                    prepaid_summary(giveaway.prize, giveaway.winner_count),
                    boosts
                );
                section = section.child(
                    Button::new(("boosts-prepaid-giveaway", index as u64))
                        .label(label)
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_giveaway_form(chat_id, Some(giveaway.clone()), window, cx);
                        })),
                );
            }
        }
        section.into_any_element()
    }
}

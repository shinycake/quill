//! "Get Boosts via Gifts" and the prepaid giveaways on the boosts page
//! (tdesktop `FillGetBoostsButton` and the "Prepaid giveaways" list in
//! `info/channel_statistics/boosts/info_boosts_inner_widget.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::giveaway_create_ui::prepaid_summary;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::RequestPurpose;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// Retry on a boost status that did not arrive.
    fn retry_boost_status(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_boost_status(chat_id);
        }
        cx.notify();
    }

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
        let details = session.groups.chat_boost_details.get(&chat_id.0);
        let prepaid = details
            .map(|status| status.prepaid_giveaways.clone())
            .unwrap_or_default();
        // The prepaid list is not "none" while the status is missing: its
        // request is out (the table says so), or it failed and Retry asks.
        let status_in_flight = session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id);
        let multiplier = quill::giveaway::BOOSTS_PER_PREMIUM;
        let mut section = div().flex().flex_col().gap_1().pt_2().child(
            Button::new("boosts-get-via-gifts")
                .label(tr(keys::BOOSTS_GET_BOOSTS))
                .small()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_giveaway_form(chat_id, None, window, cx);
                })),
        );
        section = section.child(div().text_xs().text_color(muted).child(tr_with(
            keys::Q_MONEY_GET_MORE_BOOSTS_FOR_YOUR,
            &[("0", place.0), ("1", place.1)],
        )));
        if details.is_none() {
            section = if status_in_flight {
                section.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_LOADING_BOOST_STATUS)),
                )
            } else {
                section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(muted)
                                .child(tr(keys::Q_MONEY_BOOST_STATUS_UNAVAILABLE)),
                        )
                        .child(
                            Button::new("boosts-status-retry")
                                .label(tr(keys::Q_MONEY_RETRY))
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.retry_boost_status(chat_id, cx);
                                })),
                        ),
                )
            };
        }
        if !prepaid.is_empty() {
            section = section
                .child(
                    div()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::BOOSTS_PREPAID_GIVEAWAY_TITLE)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::BOOSTS_PREPAID_GIVEAWAY_TITLE_SUBTEXT)),
                );
            for (index, giveaway) in prepaid.into_iter().enumerate() {
                let boosts = if giveaway.boost_count > 0 {
                    giveaway.boost_count
                } else {
                    giveaway.winner_count.saturating_mul(multiplier)
                };
                let label = tr_with(
                    keys::Q_MONEY_BOOSTS,
                    &[
                        (
                            "winner_count",
                            &(prepaid_summary(giveaway.prize, giveaway.winner_count)).to_string(),
                        ),
                        ("boosts", &boosts.to_string()),
                    ],
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

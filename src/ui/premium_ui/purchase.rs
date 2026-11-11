//! The Premium subscribe button (tdesktop `StartPremiumPayment`): open the
//! Premium bot from the app config and send `/start <ref>` right away, or
//! open the in-app payment form of the configured invoice link.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::premium_hub::PremiumAutoStart;
use quill::premium_hub::promo::PromoPage;
use quill::premium_hub::purchase::{PremiumPurchaseRoute, purchase_route};
use quill::state::DeepLinkAction;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// tdesktop `LookupPremiumRef` for the page the dialog is on: the
    /// feature being previewed, the limits page, or the ref the dialog was
    /// opened with.
    pub(super) fn premium_start_ref(&self) -> String {
        match self.promo_page() {
            PromoPage::Overview => self
                .session()
                .map(|s| s.payments.hub.premium_ref.clone())
                .unwrap_or_default(),
            PromoPage::Limits => "double_limits".into(),
            PromoPage::Preview { feature, .. } => feature.start_ref().unwrap_or_default().into(),
        }
    }

    /// The subscribe button. With the bot named in the app config the bot's
    /// chat opens and gets `/start <ref>`; otherwise the invoice link's
    /// payment form opens. With neither there is nothing to open.
    pub(super) fn start_premium_payment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start_ref = self.premium_start_ref();
        let Some(purchase) = self.session().map(|s| s.payments.hub.purchase.clone()) else {
            return;
        };
        match purchase_route(&purchase, &start_ref) {
            PremiumPurchaseRoute::Bot {
                username,
                start_ref,
            } => {
                self.close_premium(cx);
                let auto_start = (username.to_lowercase(), start_ref.clone());
                let sent = self.resolve_deep_link_here(DeepLinkAction::OpenUsername {
                    domain: username,
                    start_param: Some(start_ref),
                    post: None,
                    story_id: None,
                });
                if sent != Some(false) {
                    // Offline (demos) nothing is sent and nothing answers.
                    // Tied to the resolve just sent: its error drops it.
                    let generation = self.session().map_or(0, |s| s.chats_state.deep_link_seq);
                    if let Some(hub) = self.hub_mut() {
                        hub.auto_start = Some(PremiumAutoStart {
                            username: auto_start.0,
                            start_ref: auto_start.1,
                            generation,
                        });
                    }
                }
                if sent == Some(false) {
                    self.set_status_note(
                        StatusNote::error(tr(keys::Q_MONEY_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
                        cx,
                    );
                }
            }
            PremiumPurchaseRoute::Invoice { slug } => {
                self.close_premium(cx);
                self.open_invoice_link(&slug, window, cx);
            }
            PremiumPurchaseRoute::Unavailable => {
                // The app config may not have arrived yet.
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.ensure_earn_app_config();
                }
                self.set_status_note(
                    StatusNote::info(tr(keys::Q_MONEY_PREMIUM_ISNT_AVAILABLE_TO_BUY)),
                    cx,
                );
            }
        }
    }

    /// Forget the `/start` the subscribe button queued for the Premium
    /// bot: the account left the screen (its session stays in the
    /// background) before the bot's chat opened.
    pub(in crate::ui) fn drop_premium_auto_start(&mut self) {
        if let Some(hub) = self.hub_mut() {
            hub.auto_start = None;
        }
    }

    /// The Premium bot's chat just opened from the subscribe button: send
    /// `/start <ref>` without asking (tdesktop `startAutoSubmit`). Returns
    /// whether it did, so the caller skips its own prefill.
    pub(in crate::ui) fn take_premium_auto_start(
        &mut self,
        chat_id: ChatId,
        action: &DeepLinkAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let DeepLinkAction::OpenUsername { domain, .. } = action else {
            return false;
        };
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let session = &mut live.driver.session;
        let matches_bot = session
            .payments
            .hub
            .auto_start
            .as_ref()
            .is_some_and(|auto| auto.username.eq_ignore_ascii_case(domain));
        if !matches_bot {
            // Another link opened a chat: the Premium link's flow is over,
            // and its `/start` must not wait for a later visit to the bot.
            session.payments.hub.auto_start = None;
            return false;
        }
        let bot_id = match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Private { user_id }) => user_id.0,
            _ => return false,
        };
        let Some(PremiumAutoStart { start_ref, .. }) = session.payments.hub.auto_start.take()
        else {
            return false;
        };
        self.press_bot_start(chat_id, bot_id, start_ref, cx);
        true
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "purchase_tests.rs"]
mod tests;

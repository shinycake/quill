//! The Premium subscribe button (tdesktop `StartPremiumPayment`): open the
//! Premium bot from the app config and send `/start <ref>` right away, or
//! open the in-app payment form of the configured invoice link.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::premium_hub::promo::PromoPage;
use quill::premium_hub::purchase::{PremiumPurchaseRoute, feature_ref, purchase_route};
use quill::state::DeepLinkAction;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// tdesktop `LookupPremiumRef` for the page the dialog is on: the
    /// feature being previewed, the limits page, or the ref the dialog was
    /// opened with.
    pub(super) fn premium_start_ref(&self) -> String {
        let opened_with = self
            .session()
            .map(|s| s.payments.hub.premium_ref.clone())
            .unwrap_or_default();
        match self.promo_page() {
            PromoPage::Overview => opened_with,
            PromoPage::Limits => "double_limits".into(),
            PromoPage::Preview(index) => self
                .preview_feature_title(index)
                .and_then(feature_ref)
                .map(str::to_string)
                .unwrap_or(opened_with),
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
                if let Some(hub) = self.hub_mut() {
                    hub.auto_start = Some((username.to_lowercase(), start_ref.clone()));
                }
                self.close_premium(cx);
                self.note_deep_link_origin();
                if let Some(live) = self.live.as_mut() {
                    let sent = live.driver.resolve_deep_link(DeepLinkAction::OpenUsername {
                        domain: username,
                        start_param: Some(start_ref),
                        post: None,
                        story_id: None,
                    });
                    if sent.is_err() {
                        live.driver.session.payments.hub.auto_start = None;
                        self.set_status_note(
                            StatusNote::error(tr(keys::Q_MONEY_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
                            cx,
                        );
                    }
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
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case(domain));
        if !matches_bot {
            return false;
        }
        let bot_id = match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Private { user_id }) => user_id.0,
            _ => return false,
        };
        let Some((_, start_ref)) = session.payments.hub.auto_start.take() else {
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

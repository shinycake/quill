//! `ready-premium-preview`: the Premium dialog on one feature's preview over
//! the ready chat list (injected, no live Telegram). `QUILL_DEMO_PREMIUM`
//! picks the upsell: unset or `box` for the emoji status box (one Unlock
//! button), `to-buy` for the Business links preview with the plans, `gifts`
//! for the Gifts explainer, `business` for the Business hub of a free
//! account. The GPUI tests in `preview_tests.rs` drive the same pages.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::premium_hub::promo::PromoFeature;
use quill::premium_hub::{
    BusinessFeature, PremiumFeature, parse_premium_features, parse_premium_state,
};

register_demos![
    DemoSpec::chats(
        "ready-premium-preview",
        "screenshot demo — a Premium upsell opens its feature's preview (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_premium_preview),
];

impl QuillApp {
    fn demo_premium_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            let hub = &mut session.payments.hub;
            hub.premium = Some(parse_premium_features(&serde_json::json!({
                "features": [
                    {"@type": "premiumFeatureIncreasedLimits"},
                    {"@type": "premiumFeatureVoiceRecognition"},
                    {"@type": "premiumFeatureDisabledAds"},
                    {"@type": "premiumFeatureUniqueReactions"},
                    {"@type": "premiumFeatureEmojiStatus"},
                    {"@type": "premiumFeatureAccentColor"},
                ],
                "limits": [],
            })));
            hub.premium_state = Some(parse_premium_state(&serde_json::json!({
                "state": {"text": "", "entities": []},
                "payment_options": [
                    {"payment_option": {"currency": "USD", "amount": 499,
                        "discount_percentage": 0, "month_count": 1}},
                    {"payment_option": {"currency": "USD", "amount": 2999,
                        "discount_percentage": 50, "month_count": 12}},
                ],
            })));
        }
        match std::env::var("QUILL_DEMO_PREMIUM").as_deref() {
            Ok("to-buy") => self.open_premium_to_buy(BusinessFeature::AccountLinks, cx),
            Ok("gifts") => self.open_premium_at(PromoFeature::Gifts, cx),
            Ok("business") => self.open_business(window, cx),
            _ => self.open_premium_at(PremiumFeature::EmojiStatus, cx),
        }
        cx.notify();
    }
}

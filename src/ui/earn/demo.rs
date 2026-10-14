//! Screenshot demos of the revenue and affiliate pages (injected, no live
//! Telegram).

use super::fixture::*;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::earn::{EarnPanel, EarnPeer, EarnScreen};
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-earn-monetization",
        "screenshot demo — channel Monetization page"
    )
    .setup(QuillApp::demo_earn_monetization),
    DemoSpec::chats(
        "ready-earn-monetization-end",
        "screenshot demo — channel Monetization page, scrolled to the balances"
    )
    .setup(QuillApp::demo_earn_monetization_end),
    DemoSpec::chats(
        "ready-earn-learn",
        "screenshot demo — Monetization learn box"
    )
    .setup(QuillApp::demo_earn_learn),
    DemoSpec::chats(
        "ready-earn-bot-stars",
        "screenshot demo — bot Stars Balance page"
    )
    .setup(QuillApp::demo_earn_bot_stars),
    DemoSpec::chats(
        "ready-earn-affiliate-join",
        "screenshot demo — affiliate programs page"
    )
    .setup(QuillApp::demo_earn_affiliate_join),
    DemoSpec::chats(
        "ready-earn-join-box",
        "screenshot demo — join an affiliate program"
    )
    .setup(QuillApp::demo_earn_join_box),
    DemoSpec::chats(
        "ready-earn-link-box",
        "screenshot demo — affiliate referral link"
    )
    .setup(QuillApp::demo_earn_link_box),
    DemoSpec::chats(
        "ready-earn-affiliate-setup",
        "screenshot demo — bot affiliate program setup"
    )
    .setup(QuillApp::demo_earn_affiliate_setup),
];

impl QuillApp {
    /// Run a fixture on the demo session with the envelope sequence it
    /// continues.
    fn demo_earn_seed(
        &mut self,
        seed: impl FnOnce(
            &mut quill::state::Session,
            &std::sync::Arc<quill::diagnostics::MemorySink>,
            &std::sync::atomic::AtomicU64,
        ),
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            seed(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
    }

    fn demo_earn_monetization(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_earn_seed(seed_channel_monetization);
        self.open_earn(
            EarnPanel::Revenue {
                peer: EarnPeer::Chat(DEMO_CHANNEL),
                screen: EarnScreen::Monetization,
            },
            window,
            cx,
        );
    }

    fn demo_earn_monetization_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_earn_monetization(window, cx);
        self.demo_earn_scroll_to_end(cx);
    }

    /// Scroll the page far down; layout clamps it to the end.
    fn demo_earn_scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.earn.scroll.set_offset(point(px(0.), px(-100_000.)));
        cx.notify();
    }

    fn demo_earn_learn(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_earn_monetization(window, cx);
        self.earn.learn = true;
    }

    fn demo_earn_bot_stars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_earn_seed(seed_bot_stars);
        self.open_earn(
            EarnPanel::Revenue {
                peer: EarnPeer::User(OWN_BOT),
                screen: EarnScreen::Stars,
            },
            window,
            cx,
        );
    }

    fn demo_earn_affiliate_join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let peer = EarnPeer::Chat(DEMO_CHANNEL);
        self.demo_earn_seed(|session, sink, seq| seed_affiliate_join(session, sink, seq, peer));
        self.open_earn(EarnPanel::Join { peer }, window, cx);
    }

    fn demo_earn_join_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let peer = EarnPeer::User(777);
        self.demo_earn_seed(|session, sink, seq| {
            seed_affiliate_join(session, sink, seq, peer);
            open_join_box(session);
        });
        self.open_earn(EarnPanel::Join { peer }, window, cx);
    }

    fn demo_earn_link_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let peer = EarnPeer::Chat(DEMO_CHANNEL);
        self.demo_earn_seed(|session, sink, seq| {
            seed_affiliate_join(session, sink, seq, peer);
            open_link_box(session, peer);
        });
        self.open_earn(EarnPanel::Join { peer }, window, cx);
    }

    fn demo_earn_affiliate_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_earn_seed(seed_affiliate_setup);
        self.open_earn(
            EarnPanel::Setup {
                bot_user_id: OWN_BOT,
            },
            window,
            cx,
        );
    }
}

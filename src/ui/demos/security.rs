//! Screenshot demos: security.

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_sessions, demo_storage_stats};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::settings::ThemeChoice;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-marketplace-gift",
        "screenshot demo — collectible gift quote (no purchase)"
    )
    .setup(QuillApp::demo_ready_marketplace_gift),
    // Slice A3: Active Sessions overlay (injected, no live Telegram) —
    // fixture `getActiveSessions` sessions (current device + two other
    // sessions + one incomplete login attempt), dialog open.
    DemoSpec::chats(
        "ready-sessions",
        "screenshot demo — active sessions (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_sessions),
    // Appearance dialog with the Spelling / Check spelling row visible.
    DemoSpec::chat_list("ready-spellcheck-toggle").setup(QuillApp::demo_appearance),
    // Phase S2: storage-usage overlay (injected, no live Telegram) —
    // fixture `getStorageStatistics` stats with the "Secret media and
    // files" category, dialog open.
    DemoSpec::chats(
        "ready-storage-usage",
        "screenshot demo — data & storage (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_storage_usage),
];

impl QuillApp {
    fn demo_ready_marketplace_gift(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            let quote=quill::marketplace::GiftQuote::parse(&serde_json::json!({"name":"PlushPepe-123","title":"Plush Pepe","resale_parameters":{"star_count":25,"gram_cent_count":0,"gram_only":false}})).unwrap();
            session.gift_text_length_max = Some(128);
            session.marketplace_gift = Some(quill::marketplace::GiftPurchase {
                chat_id: ChatId(11),
                recipient: quill::telegram::envelope::MessageSender::User { user_id: 11 },
                recipient_name: "Demo chat A".into(),
                requested_name: quote.name.clone(),
                price: quote.stars,
                quote: Some(quote),
                loading: false,
                sending: false,
                completed: false,
                note: Some("Injected quote — no purchase is sent in this demo.".into()),
            });
        }
        self.payments
            .marketplace_name_input
            .update(cx, |input, cx| input.set_value("PlushPepe-123", window, cx));
        self.payments
            .marketplace_comment_input
            .update(cx, |input, cx| {
                input.set_value("A little gift for you 🎁", window, cx)
            });
        self.payments.marketplace_open = true;
    }

    fn demo_ready_sessions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A3: Active Sessions fixture — fixture sessions (current
        // device, two other sessions, one incomplete login attempt) with
        // the overlay open (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            session.sessions = Some(demo_sessions());
            session.sessions_loading = false;
            session.sessions_error = None;
        }
        self.privacy.sessions_open = true;
        if std::env::var_os("QUILL_DEMO_DEVICE_LINK").is_some() {
            self.privacy.device_login_qr =
                Some(zeroize::Zeroizing::new("tg://login?token=AQID".into()));
        }
        self.connection.status_note = "screenshot demo — active sessions".into();
    }

    fn demo_ready_storage_usage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice S4: Data & Storage fixture — fixture stats (including the
        // secret category and per-chat rows) plus seeded per-network
        // download settings, with the dialog open (injected, no live
        // Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            session.storage_stats = Some(demo_storage_stats());
            session.storage_stats_loading = false;
            session.data_storage = demo_data_storage_prefs();
            session.storage_freed = Some(54_525_952);
        }
        self.settings.storage_usage_open = true;
        self.connection.status_note = "screenshot demo — data & storage".into();
    }

    fn demo_appearance(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Settings → Appearance: the Appearance dialog open over the
        // ReadyChats fixture (injected, no live Telegram). Non-default
        // values so the screenshot shows the slice live: dark theme,
        // blue accent, dark wallpaper, 16px message text. They are only
        // in-memory for the demo — `apply_appearance` (end of this fn)
        // picks them up; nothing is persisted.
        self.chat_prefs.spellcheck_enabled = true;
        // stories-high-contrast: `QUILL_DEMO_THEME=high-contrast`
        // captures the dialog with the HC theme selected.
        self.appearance.theme =
            if std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast") {
                ThemeChoice::HighContrast
            } else {
                ThemeChoice::Dark
            };
        self.appearance.accent_rgb = 0x2f81f7;
        self.appearance.wallpaper_rgb = Some(0x0e1621);
        self.appearance.font_size_px = 16;
        self.settings.appearance_open = true;
        self.connection.status_note = "screenshot demo — appearance settings".into();
    }
}

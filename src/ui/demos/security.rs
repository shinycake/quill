//! Screenshot demos: security.

use super::attachments;
use crate::ui::app::QuillApp;
use crate::ui::contacts::apply_ready_contacts;
use crate::ui::demo::{
    demo_password_state_manage, demo_password_state_pending, demo_sessions,
    demo_star_subscriptions, demo_storage_stats, demo_websites,
};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::secret_chats::apply_ready_key_verification;
use crate::ui::secret_chats::{apply_ready_secret_chat, apply_ready_self_destruct};
use crate::ui::wallpaper::apply_demo_wallpapers;
use crate::ui::*;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::ids::ChatId;
use quill::settings::ThemeChoice;
use quill::telegram::requests::SelfDestructSend;
use std::sync::atomic::Ordering;

register_demos![
    // Batch 4/6: Two-step "Forgot password?" code step (injected, no live Telegram).
    DemoSpec::chats(
        "ready-2fa-forgot",
        "screenshot demo — ready-2fa-forgot (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready2fa_forgot),
    // Slice A2: two-step verification overlay — password set with
    // recovery email (injected `passwordState`, no live Telegram).
    DemoSpec::chats(
        "ready-2fa-manage",
        "screenshot demo — two-step verification (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready2fa_manage),
    // Batch 4/6: Two-step password reset waiting period (injected, no live Telegram).
    DemoSpec::chats(
        "ready-2fa-reset",
        "screenshot demo — ready-2fa-reset (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready2fa_reset),
    // Slice A9: account lifecycle dialog — injected `accountTtl` (180
    // days) + `passwordState` with a password set, dialog open (no live
    // Telegram).
    DemoSpec::chats(
        "ready-account",
        "screenshot demo — account lifecycle (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_account_lifecycle),
    // Slice parity:auth-multi-account (UI): the Accounts dialog open
    // over the ReadyChats fixture (injected, no live Telegram). The
    // account list reads the real local registry (read-only).
    DemoSpec::chat_list("ready-accounts").setup(QuillApp::demo_ready_accounts),
    // Settings → Appearance: the Appearance dialog open over the
    // ReadyChats fixture (injected, no live Telegram).
    DemoSpec::chat_list("ready-appearance").setup(|app, window, cx| app.demo_appearance(
        AppearanceDemo::Appearance,
        window,
        cx
    )),
    // Appearance cluster: system accent, font family and the Battery and
    // animations switches.
    DemoSpec::chat_list("ready-appearance-power").setup(QuillApp::demo_ready_appearance_power),
    // Appearance slice: Appearance with Telegram wallpapers and interface scale.
    DemoSpec::chat_list("ready-appearance-wallpapers")
        .setup(QuillApp::demo_ready_appearance_wallpapers),
    // A `bg/` link preview (pattern wallpaper).
    DemoSpec::ready(
        "ready-background-link",
        crate::ui::chat_look_demo::seed_chat_look_link,
        "screenshot demo — wallpaper link preview"
    )
    .setup(QuillApp::demo_ready_background_link),
    // Per-chat theme and wallpaper picker open over a private chat.
    DemoSpec::ready(
        "ready-chat-look",
        crate::ui::chat_look_demo::seed_chat_look_picker,
        "screenshot demo — chat theme and wallpaper picker"
    )
    .setup(QuillApp::demo_ready_chat_look),
    // A private chat wearing an emoji theme and its own pattern wallpaper.
    DemoSpec::ready(
        "ready-chat-theme",
        crate::ui::chat_look_demo::seed_chat_look_themed,
        "screenshot demo — chat theme and wallpaper"
    ),
    // Appearance → Spelling with the Manage dictionaries list open
    // (fixture rows: enabled, installed, downloading 42%, failed, available).
    DemoSpec::chat_list("ready-dictionaries").setup(|app, window, cx| app.demo_appearance(
        AppearanceDemo::Dictionaries,
        window,
        cx
    )),
    // Phase B2: key verification UI (injected, no live Telegram) — the
    // same Ready secret chat as `ReadySecretChat` but with a real
    // 36-byte `key_hash` (deterministic fixture), and the partner's
    // info panel open showing the "Encryption key" 12×12 fingerprint
    // grid plus the verification copy.
    DemoSpec::chats(
        "ready-key-verification",
        "screenshot demo — secret chat key verification (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_key_verification),
    // Slice `parity:platform-custom-keybindings`: the Appearance dialog
    // open on the Keyboard shortcuts section (injected, no live Telegram).
    DemoSpec::chat_list("ready-keybindings").setup(|app, window, cx| app.demo_appearance(
        AppearanceDemo::Keybindings,
        window,
        cx
    )),
    // Batch 4/6: Local storage: ticked types, clear confirmation and limits (injected, no live Telegram).
    DemoSpec::chats(
        "ready-local-storage",
        "screenshot demo — ready-local-storage (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_local_storage),
    // Local passcode: the lock screen after a wrong passcode.
    DemoSpec::chat_list("ready-lock-screen").setup(|app, window, cx| app.demo_passcode(
        PasscodeDemo::LockScreen,
        window,
        cx
    )),
    // Batch 4/6: Login email code step (injected, no live Telegram).
    DemoSpec::chats(
        "ready-login-email",
        "screenshot demo — ready-login-email (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_login_email),
    // Batch 4/6: "New Login Prevented" box after "No, it's not me!" (injected, no live Telegram).
    DemoSpec::chats(
        "ready-login-prevented",
        "screenshot demo — ready-login-prevented (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_login_prevented),
    DemoSpec::chats(
        "ready-marketplace-gift",
        "screenshot demo — collectible gift quote (no purchase)"
    )
    .setup(QuillApp::demo_ready_marketplace_gift),
    // Batch 4/6: New-login alert strip over the chat list (injected unconfirmed session, no live Telegram).
    DemoSpec::chats(
        "ready-new-login",
        "screenshot demo — ready-new-login (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_new_login),
    // Local passcode: the create form with a mismatch error.
    DemoSpec::chat_list("ready-passcode-create").setup(|app, window, cx| app.demo_passcode(
        PasscodeDemo::PasscodeCreate,
        window,
        cx
    )),
    // Local passcode: the settings dialog with a passcode set (auto-lock,
    // Touch ID rows).
    DemoSpec::chat_list("ready-passcode-settings").setup(|app, window, cx| app.demo_passcode(
        PasscodeDemo::PasscodeSettings,
        window,
        cx
    )),
    // Slice A2: two-step verification overlay — recovery email pending
    // confirmation (injected `passwordState` with
    // `recovery_email_address_code_info`, no live Telegram).
    DemoSpec::chats(
        "ready-recovery-email",
        "screenshot demo — recovery email pending (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_recovery_email),
    // Phase S2: inline-bot warning banner (injected, no live Telegram)
    // — the Ready secret chat fixture with a pending
    // `SwitchInline` alert above the composer.
    DemoSpec::chats(
        "ready-secret-bot-alert",
        "screenshot demo — inline-bot warning (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_secret_bot_alert),
    // Phase B1: secret chat lifecycle (injected, no live Telegram) — a
    // Ready secret chat (id 41) with Zed (user 41): `updateSecretChat`
    // (Ready) → `updateNewChat` (`chatTypeSecret`) → history, opened
    // with three E2E messages. The chat-list row shows the 🔒 badge and
    // the composer is live (a Pending chat would show "Waiting for Zed
    // to come online…" and a Closed chat "Secret chat closed" instead).
    DemoSpec::chats(
        "ready-secret-chat",
        "screenshot demo — secret chat lifecycle (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_secret_chat),
    // Phase S1: "New secret chat" contact picker (injected, no live
    // Telegram) — the Ready secret chat fixture plus the contacts
    // fixture, with the sidebar "🔒 New secret chat" picker open.
    // `session.contacts` is assigned directly (the fixture equivalent
    // of a `getContacts` answer) so the picker has eligible rows.
    DemoSpec::chats(
        "ready-secret-picker",
        "screenshot demo — new secret chat picker (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_secret_picker),
    // Phase B3: self-destructing media (injected, no live Telegram) —
    // a Ready *private* (1:1 cloud) chat with Zed: an incoming photo
    // with a live 60s `messageSelfDestructTypeTimer` countdown, an
    // outgoing `messageSelfDestructTypeImmediately` ("view once")
    // photo, and a pending photo attachment with the composer's timer
    // picker on 30s. Private chat — not a secret chat — because TDLib
    // only accepts per-media `self_destruct_type` in
    // `chatTypePrivate` chats (schema 1.8.67 lines 6117/6128,
    // "private chats only").
    DemoSpec::chats(
        "ready-self-destruct",
        "screenshot demo — self-destructing media (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_self_destruct)
    // Phase B3: pending photo for the self-destruct picker demo.
    .attachments(|| attachments(&[("demo-thumb.png", AttachmentKind::Photo)])),
    // Batch 4/6: Server service notification popup (injected, no live Telegram).
    DemoSpec::chats(
        "ready-service-notice",
        "screenshot demo — ready-service-notice (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_service_notice),
    DemoSpec::chats(
        "ready-session-toggles",
        "screenshot demo — session acceptance toggles (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_session_toggles),
    // Slice A3: Active Sessions overlay (injected, no live Telegram) —
    // fixture `getActiveSessions` sessions (current device + two other
    // sessions + one incomplete login attempt), dialog open.
    DemoSpec::chats(
        "ready-sessions",
        "screenshot demo — active sessions (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_sessions),
    // Appearance dialog with the Spelling / Check spelling row visible.
    DemoSpec::chat_list("ready-spellcheck-toggle").setup(|app, window, cx| app.demo_appearance(
        AppearanceDemo::SpellcheckToggle,
        window,
        cx
    )),
    // Phase S2: storage-usage overlay (injected, no live Telegram) —
    // fixture `getStorageStatistics` stats with the "Secret media and
    // files" category, dialog open.
    DemoSpec::chats(
        "ready-storage-usage",
        "screenshot demo — data & storage (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_storage_usage),
    // Slice `parity:bots-payment-recurring`: the ⭐ Subscriptions dialog
    // open over the ReadyChats fixture — fixture `starSubscriptions`
    // (active channel, canceled bot, expired channel rows), dialog open
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-subscriptions",
        "screenshot demo — ⭐ subscriptions (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_subscriptions),
    // Batch 4/6: Terms of Service prompt with the age check (injected, no live Telegram).
    DemoSpec::chats(
        "ready-terms",
        "screenshot demo — ready-terms (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_terms),
    // Slice A4: Connected Websites overlay (injected, no live Telegram)
    // — fixture `getConnectedWebsites` websites, dialog open.
    DemoSpec::chats(
        "ready-web-sessions",
        "screenshot demo — connected websites (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_web_sessions),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppearanceDemo {
    Appearance,
    SpellcheckToggle,
    Dictionaries,
    Keybindings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PasscodeDemo {
    PasscodeSettings,
    PasscodeCreate,
    LockScreen,
}

impl QuillApp {
    fn demo_ready2fa_forgot(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 6: "Forgot password?" code step (injected).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.password_state = Some(demo_password_state_manage());
            session.password_state_loading = false;
            session.twofa_flow.recovery_code_sent_to = Some("i***@example.com".into());
        }
        self.twofa.view = TwofaView::Recover;
        self.twofa.open = true;
    }

    fn demo_ready2fa_manage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A2: 2FA overlay fixture — password set with recovery
        // email (injected `passwordState`, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.password_state = Some(demo_password_state_manage());
            session.password_state_loading = false;
        }
        self.twofa.open = true;
        self.status_note = "screenshot demo — two-step verification".into();
    }

    fn demo_ready2fa_reset(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 6: reset waiting period (injected).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            let mut state = demo_password_state_manage();
            state.has_recovery_email_address = false;
            state.pending_reset_date =
                ((quill::state::unix_ms_now() / 1000) + 5 * 86_400 + 3_600) as i32;
            session.password_state = Some(state);
            session.password_state_loading = false;
        }
        self.twofa.view = TwofaView::Recover;
        self.twofa.open = true;
    }

    fn demo_ready_account_lifecycle(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A9: account lifecycle fixture — injected `accountTtl`
        // (180 days) + `passwordState` with a password set (no live
        // Telegram), dialog open.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.account_ttl_days = Some(180);
            session.default_auto_delete_secs = Some(604_800);
            session.account_ttl_loading = false;
            session.password_state = Some(demo_password_state_manage());
            session.password_state_loading = false;
        }
        self.account_lifecycle.open = true;
        // Slice auth-logout-warning: arm the logout confirm so the
        // screenshot shows the SignOutHint2 warning.
        self.account_lifecycle.confirm_logout = true;
        self.status_note = "screenshot demo — account lifecycle".into();
    }

    fn demo_ready_accounts(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.accounts_ui.open = true;
        self.status_note = "screenshot demo — accounts".into();
    }

    fn demo_ready_appearance_power(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Appearance cluster: the system accent, a font family and the
        // power-saving switches, with fixture values (the OS accent and the
        // installed fonts differ per machine).
        self.system_accent = Some(0xa550a7);
        self.system_accent_probed = true;
        self.appearance.system_accent = true;
        self.appearance.font_family = "Georgia".into();
        self.font_picker.update(cx, |picker, cx| {
            picker.set_selected_value(&SharedString::from("Georgia"), window, cx)
        });
        self.appearance.power_saving = quill::power_saving::Flag::StickersChat.bit()
            | quill::power_saving::Flag::ChatSpoiler.bit()
            | quill::power_saving::Flag::Calls.bit();
        self.appearance_open = true;
        self.appearance_power_screenshot = true;
        self.status_note = "screenshot demo — appearance: accent, font and power saving".into();
    }

    fn demo_ready_appearance_wallpapers(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Appearance slice: interface scale and Telegram wallpapers.
        if let Some(session) = self.demo_session.as_mut() {
            apply_demo_wallpapers(session);
        }
        self.appearance.interface_scale_pct = 125;
        self.appearance.telegram_wallpaper = true;
        self.appearance_open = true;
        self.status_note = "screenshot demo — appearance: scale and wallpapers".into();
    }

    fn demo_ready_background_link(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.chat_look_dialog = Some(crate::ui::chat_look_ui::ChatLookDialog {
            target: crate::ui::chat_look_ui::LookTarget::Link {
                name: "doodles".into(),
            },
            theme: None,
            background: None,
            remove_wallpaper: false,
            both: false,
            awaiting: None,
        });
    }

    fn demo_ready_chat_look(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Per-chat theme and wallpaper: the picker, the themed chat, and the
        // `bg/` link preview over the ReadyChats fixture.
        self.chat_look_dialog = Some(crate::ui::chat_look_ui::ChatLookDialog {
            target: crate::ui::chat_look_ui::LookTarget::Chat(11),
            theme: Some("🌷".into()),
            background: Some(6),
            remove_wallpaper: false,
            both: false,
            awaiting: None,
        });
    }

    fn demo_ready_key_verification(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase B2: key verification fixture — the Ready secret chat with
        // a real 36-byte key_hash and Zed's info panel open on the
        // "Encryption key" fingerprint grid.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_key_verification(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "secret chat key verification — compare with your contact's device".into();
    }

    fn demo_ready_local_storage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 6: local storage fixture — two ticked types with the clear
        // confirmation open, limits applied (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.storage_stats = Some(demo_storage_stats());
            session.storage_stats_loading = false;
            session.data_storage = demo_data_storage_prefs();
            for (name, value) in
                quill::storage_limits::options_for(Some(2 * 1024 * 1024 * 1024), Some(31 * 86_400))
            {
                session.storage_limits.apply_option(
                    name,
                    &match value {
                        quill::storage_limits::StorageOptionValue::Boolean(on) => {
                            quill::telegram::envelope::OptionValue::Boolean(on)
                        }
                        quill::storage_limits::StorageOptionValue::Integer(n) => {
                            quill::telegram::envelope::OptionValue::Integer(n)
                        }
                    },
                );
            }
        }
        self.storage_selected.insert("fileTypePhoto");
        self.storage_selected.insert("fileTypeVideo");
        self.storage_confirm = Some(StorageClear::Selected);
        self.storage_usage_open = true;
    }

    fn demo_ready_login_email(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 6: login email code step (injected).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            let mut state = demo_password_state_manage();
            state.login_email_address_pattern = "i***@example.com".into();
            session.password_state = Some(state);
            session.password_state_loading = false;
            session.twofa_flow.login_email_code_sent_to = Some("m***@example.com".into());
        }
        self.twofa.view = TwofaView::LoginEmail;
        self.twofa.open = true;
    }

    fn demo_ready_login_prevented(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 4: "New Login Prevented" box (injected, no live Telegram).
        self.login_prevented = Some(vec!["Berlin, Germany (Pixel 9)".into()]);
    }

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
        self.marketplace_name_input
            .update(cx, |input, cx| input.set_value("PlushPepe-123", window, cx));
        self.marketplace_comment_input.update(cx, |input, cx| {
            input.set_value("A little gift for you 🎁", window, cx)
        });
        self.marketplace_open = true;
    }

    fn demo_ready_new_login(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 4: new-login alert fixture — an unconfirmed Android login
        // resolved from the sessions list (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.unconfirmed_count = 1;
            session.notices.unconfirmed_entries = vec![quill::state::UnconfirmedEntry {
                id: 77,
                device: "Pixel 9".into(),
                location: "Berlin, Germany".into(),
            }];
        }
    }

    fn demo_ready_recovery_email(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A2: 2FA overlay fixture — recovery email pending
        // confirmation (injected `passwordState`, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.password_state = Some(demo_password_state_pending());
            session.password_state_loading = false;
        }
        self.twofa.open = true;
        self.status_note = "screenshot demo — recovery email pending".into();
    }

    fn demo_ready_secret_bot_alert(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase S2: inline-bot warning fixture — the Ready secret chat
        // with a stashed `SwitchInline` query, so the warning banner
        // renders above the composer (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
        }
        self.pending_inline_bot_alert = Some("@gif cats".to_string());
        self.status_note = "screenshot demo — inline-bot warning in secret chat".into();
    }

    fn demo_ready_secret_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase B1: secret chat lifecycle fixture — a Ready secret chat
        // with Zed, opened with E2E history and the composer live.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("this goes through the E2E session…", window, cx);
        });
        self.status_note = "secret chat — Ready, 🔒 badge in the chat list".into();
    }

    fn demo_ready_secret_picker(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase S1: "New secret chat" picker fixture — the secret chat
        // fixture plus contacts, with the picker open. `contacts` is
        // assigned directly (fixture equivalent of a `getContacts`
        // answer); the picker rows come from `contact_rows()` through the
        // real eligibility gate.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_secret_chat(session, &self.demo_sink, &self.demo_seq);
            apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
            session.contacts = Some(vec![31, 33]);
            // `apply_ready_contacts` opens the contact info panel for
            // its own demo; the picker screenshot wants it closed.
            session.open_info_panel = None;
        }
        self.new_secret_picker_open = true;
        self.status_note = "screenshot demo — new secret chat picker".into();
    }

    fn demo_ready_self_destruct(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Phase B3: self-destructing media fixture — a private chat with
        // Zed carrying a live-timer incoming photo and a view-once
        // outgoing photo; the composer's pending photo attachment has
        // the picker pre-set to 30s.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_self_destruct(session, &self.demo_sink, &self.demo_seq);
        }
        self.composer_self_destruct = Some(SelfDestructSend::Timer(30));
        self.status_note =
            "screenshot demo — self-destructing media · picker on 30s (injected, no live Telegram)"
                .into();
    }

    fn demo_ready_service_notice(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 4: server service notification popup (injected).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.service.push_back(quill::state::ServiceNotice {
                    kind: String::new(),
                    text: "Your Telegram Premium subscription ends in 3 days. Renew it to keep your extra features.".into(),
                });
        }
    }

    fn demo_ready_session_toggles(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A4: session acceptance toggles fixture — the same fixture
        // sessions (varied Secret Chats / Calls flags) with the sessions
        // overlay open so the per-row direct toggles are visible
        // (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.sessions = Some(demo_sessions());
            session.sessions_loading = false;
            session.sessions_error = None;
        }
        self.sessions_open = true;
        self.status_note = "screenshot demo — session acceptance toggles".into();
    }

    fn demo_ready_sessions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A3: Active Sessions fixture — fixture sessions (current
        // device, two other sessions, one incomplete login attempt) with
        // the overlay open (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.sessions = Some(demo_sessions());
            session.sessions_loading = false;
            session.sessions_error = None;
        }
        self.sessions_open = true;
        if std::env::var_os("QUILL_DEMO_DEVICE_LINK").is_some() {
            self.device_login_qr = Some(zeroize::Zeroizing::new("tg://login?token=AQID".into()));
        }
        self.status_note = "screenshot demo — active sessions".into();
    }

    fn demo_ready_storage_usage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice S4: Data & Storage fixture — fixture stats (including the
        // secret category and per-chat rows) plus seeded per-network
        // download settings, with the dialog open (injected, no live
        // Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.storage_stats = Some(demo_storage_stats());
            session.storage_stats_loading = false;
            session.data_storage = demo_data_storage_prefs();
            session.storage_freed = Some(54_525_952);
        }
        self.storage_usage_open = true;
        self.status_note = "screenshot demo — data & storage".into();
    }

    fn demo_ready_subscriptions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice `parity:bots-payment-recurring`: Subscriptions fixture —
        // fixture `starSubscriptions` with the dialog open (injected, no
        // live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.star_subscriptions = Some(demo_star_subscriptions());
            session.star_subscriptions_loading = false;
            session.subscriptions_open = true;
        }
        self.status_note = "screenshot demo — ⭐ subscriptions".into();
    }

    fn demo_ready_terms(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Batch 4: terms of service prompt with the age check (injected).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.notices.terms = Some(quill::telegram::envelope::TermsOfService {
                    id: "tos-2026".into(),
                    text: "1. Telegram is a cloud service. Your messages, media and files are stored on our servers so you can reach them from any device.\n\n2. Do not use Telegram to spam, scam or harm others, and do not promote violence or sell illegal goods.\n\n3. We do not use your data for ad targeting. You can adjust how your data is used in Privacy & Security settings.\n\nBy continuing you accept these updated terms.".into(),
                    min_user_age: 16,
                    show_popup: true,
                });
        }
    }

    fn demo_ready_web_sessions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A4: Connected Websites fixture — fixture websites with
        // the overlay open (injected, no live Telegram).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            session.connected_websites = Some(demo_websites());
            session.connected_websites_loading = false;
            session.websites_error = None;
        }
        self.websites_open = true;
        self.status_note = "screenshot demo — connected websites".into();
    }

    fn demo_appearance(
        &mut self,
        demo: AppearanceDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Settings → Appearance: the Appearance dialog open over the
        // ReadyChats fixture (injected, no live Telegram). Non-default
        // values so the screenshot shows the slice live: dark theme,
        // blue accent, dark wallpaper, 16px message text. They are only
        // in-memory for the demo — `apply_appearance` (end of this fn)
        // picks them up; nothing is persisted.
        if matches!(
            demo,
            AppearanceDemo::SpellcheckToggle | AppearanceDemo::Dictionaries
        ) {
            self.chat_prefs.spellcheck_enabled = true;
        }
        if matches!(demo, AppearanceDemo::Dictionaries) {
            self.dict_manager = crate::ui::spell_dictionaries::DictManager::demo();
        }
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
        self.appearance_open = true;
        self.keybindings_screenshot = matches!(demo, AppearanceDemo::Keybindings);
        self.status_note = if self.keybindings_screenshot {
            "screenshot demo — keyboard shortcuts".into()
        } else {
            "screenshot demo — appearance settings".into()
        };
    }

    fn demo_passcode(&mut self, demo: PasscodeDemo, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice parity:auth-multi-account (UI): the Accounts dialog open
        // over the ReadyChats fixture (injected, no live Telegram). The
        // list reads the real local registry, read-only — nothing is
        // added, switched, or removed by the fixture.
        self.passcode_ui.fixture(
            !matches!(demo, PasscodeDemo::PasscodeCreate),
            matches!(demo, PasscodeDemo::LockScreen),
            matches!(demo, PasscodeDemo::LockScreen).then_some("Wrong passcode"),
        );
        self.passcode_ui.autolock_secs = 300;
        self.passcode_ui.system_unlock = true;
        if matches!(demo, PasscodeDemo::PasscodeSettings) {
            self.passcode_ui.open = true;
        }
        if matches!(demo, PasscodeDemo::PasscodeCreate) {
            self.passcode_ui.open = true;
            self.passcode_ui.view = crate::ui::passcode::PasscodeView::Create;
            self.passcode_ui.error = Some("Passcodes are different".into());
        }
        self.status_note = "screenshot demo — local passcode".into();
    }
}

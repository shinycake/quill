//! Where the account-level sync updates show up: the frozen-account
//! banner and info dialog (tdesktop `FrozenWriteRestriction` /
//! `FrozenInfoBox`), the live-location strip, the free transcription
//! counter and the age verification prompt (tdesktop
//! `ShowAgeVerification`).

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::{
    age_verification_about, age_verification_bot_url, freeze_deadline_label, live_left_label,
    live_strip_label, speech_trial_hint,
};

/// The free-transcription line the voice rows show next to "Transcribe".
/// Set before each frame from the session, read by the row builders.
#[derive(Default)]
pub(super) struct SpeechTrialHint(pub Option<SharedString>);

impl Global for SpeechTrialHint {}

/// How many running shares the strip lists before it summarizes.
const LIVE_STRIP_ROWS: usize = 3;

/// The copy of the three paragraphs in the frozen-account dialog.
const FROZEN_ROWS: [(&str, &str); 2] = [
    (
        "Violation of Terms",
        "Your account was frozen for breaking Telegram's Terms and Conditions.",
    ),
    (
        "Read-Only Mode",
        "You can access your account but can't send messages or take actions.",
    ),
];

impl QuillApp {
    /// Publish the free transcription counter for the voice rows.
    pub(super) fn sync_speech_trial_hint(&self, cx: &mut Context<Self>) {
        let hint = self
            .session()
            .and_then(|s| s.sync.speech_trial)
            .map(|trial| {
                SharedString::from(speech_trial_hint(&trial, quill::local_time::now_unix()))
            });
        if cx.try_global::<SpeechTrialHint>().map(|h| &h.0) != Some(&hint) {
            cx.set_global(SpeechTrialHint(hint));
        }
    }

    /// "Your account is frozen!" strip above the chat list (tdesktop's
    /// frozen account bar); a click opens the details.
    pub(crate) fn frozen_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.session()?.sync.freeze.as_ref()?;
        Some(
            div()
                .id("frozen-account-banner")
                .role(Role::Button)
                .aria_label("Your account is frozen. View details")
                .tab_index(0)
                .cursor_pointer()
                .w_full()
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().danger.opacity(0.14))
                .child(
                    div()
                        .font_semibold()
                        .text_sm()
                        .child("Your account is frozen!"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Click to view details \u{203a}"),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.account.freeze_info_open = true;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// One strip while a live location of ours is running: where, how
    /// long is left, whether someone looked, and Stop.
    pub(crate) fn live_share_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let now = quill::local_time::now_unix();
        let shares: Vec<_> = session.sync.live_shares_at(now).copied().collect();
        if shares.is_empty() {
            return None;
        }
        let mut strip = div()
            .id("live-share-strip")
            .role(Role::Group)
            .aria_label("Live location sharing")
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().primary.opacity(0.1))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(live_strip_label(shares.len())),
            );
        for (index, share) in shares.iter().take(LIVE_STRIP_ROWS).enumerate() {
            let title = session
                .chats
                .get(&share.chat_id.0)
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| "Chat".to_string());
            let mut detail = live_left_label(share.expires_at, now);
            if share.viewed {
                detail.push_str(" \u{b7} Viewed");
            }
            let (chat_id, message_id) = (share.chat_id, share.message_id);
            strip = strip.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(title))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(detail),
                            ),
                    )
                    .child(
                        Button::new(("live-share-stop", index as u64))
                            .label("Stop")
                            .small()
                            .outline()
                            .accessibility_label("Stop sharing live location")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.stop_live_location(chat_id, message_id, cx);
                            })),
                    ),
            );
        }
        if shares.len() > LIVE_STRIP_ROWS {
            strip = strip.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("and {} more", shares.len() - LIVE_STRIP_ROWS)),
            );
        }
        Some(strip.into_any_element())
    }

    /// Dialog body for the frozen account (tdesktop `FrozenInfoBox`).
    pub(crate) fn frozen_dialog_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement, AnyElement, bool) {
        let info = self.session().and_then(|s| s.sync.freeze.clone());
        let Some(info) = info else {
            return (
                "".into(),
                div().into_any_element(),
                div().into_any_element(),
                false,
            );
        };
        let deadline = freeze_deadline_label(&info, quill::local_time::now_unix());
        let appeal =
            format!("Appeal via @SpamBot before {deadline}, or your account will be deleted.");
        let row = |title: &str, text: String, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(title.to_string()))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text),
                )
        };
        let mut body = div().flex().flex_col().gap_3();
        for (title, text) in FROZEN_ROWS {
            body = body.child(row(title, text.to_string(), cx));
        }
        body = body.child(row("Appeal Before Deactivation", appeal, cx));
        let url = info.appeal_link.clone();
        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("frozen-close")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.account.freeze_info_open = false;
                        cx.notify();
                    })),
            )
            .when(!url.is_empty(), |this| {
                this.child(
                    Button::new("frozen-appeal")
                        .label("Submit an Appeal")
                        .primary()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_message_url(&url, cx);
                        })),
                )
            });
        (
            "Your Account is Frozen".into(),
            body.into_any_element(),
            footer.into_any_element(),
            false,
        )
    }

    /// Dialog body for age verification (tdesktop `ShowAgeVerification`).
    pub(crate) fn age_verify_dialog_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement, AnyElement, bool) {
        let params = self.session().and_then(|s| s.sync.age_verification.clone());
        let Some(params) = params else {
            return (
                "".into(),
                div().into_any_element(),
                div().into_any_element(),
                false,
            );
        };
        let bot_url = age_verification_bot_url(&params);
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(age_verification_about(&params)))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "This is a one-time process using your phone's camera. Open the verification bot in Telegram for Android or iOS. Your selfie will not be stored by Telegram.",
                    ),
            );
        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("age-verify-close")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.account.age_verify_open = false;
                        cx.notify();
                    })),
            )
            .when_some(bot_url, |this, url| {
                this.child(
                    Button::new("age-verify-open")
                        .label("Verify My Age")
                        .primary()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.account.age_verify_started = true;
                            this.open_message_url(&url, cx);
                        })),
                )
            });
        (
            "Age Verification".into(),
            body.into_any_element(),
            footer.into_any_element(),
            false,
        )
    }

    /// The "Show 18+ Content" switch. Turning it on while the server asks
    /// for age verification opens the prompt first (tdesktop
    /// `ShowAgeVerificationRequired`); once the user has started the
    /// verification, the server decides.
    pub(super) fn toggle_sensitive_content(&mut self, on: bool, cx: &mut Context<Self>) {
        let needs_verification = self
            .session()
            .is_some_and(|s| s.sync.age_verification.is_some());
        if quill::state::age_gate_blocks(on, needs_verification, self.account.age_verify_started) {
            self.account.age_verify_open = true;
            cx.notify();
        } else {
            self.set_sensitive_content(on, cx);
        }
    }
}

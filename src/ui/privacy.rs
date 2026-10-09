//! Settings → Privacy slice (TGX `SettingsPrivacyController`): the
//! privacy overlay, per-rule editor, always/never exception lists, the
//! read-date toggle, and the blocked-users section. Server state flows
//! through TDLib `getUserPrivacySettingRules` / `setUserPrivacySettingRules`
//! (+ read-date and blocked-sender constructors); unknown rule types
//! round-trip untouched via `PrivacyRuleDetail::extra_rules`.

use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::state::{ContactRow, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::requests::{CallPrivacySetting, PrivacyWho};
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Slice S3: Privacy rule editor target — one of the five
/// `PrivacySettingKey` rules, or one of the two call settings (which
/// reuse the Phase C2i `call_privacy_*` plumbing instead of the new
/// per-key state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrivacyEditorTarget {
    Rule(PrivacySettingKey),
    CallAllow,
    CallP2P,
    /// B13: "Who can message me" (`newChatPrivacySettings`).
    NewChat,
    /// B13: "File open confirmations" (extension whitelist + IP warning).
    FileOpen,
}

impl PrivacyEditorTarget {
    fn label(self) -> &'static str {
        match self {
            PrivacyEditorTarget::Rule(key) => key.label(),
            PrivacyEditorTarget::CallAllow => "Who can call me",
            PrivacyEditorTarget::CallP2P => "Peer-to-peer calls",
            PrivacyEditorTarget::NewChat => "Who can message me",
            PrivacyEditorTarget::FileOpen => "File open confirmations",
        }
    }
}

/// Slice S3: which exception list of a rule is being viewed
/// (TGX `SettingsPrivacyKeyController` always-allow / never-allow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrivacyExceptionKind {
    Always,
    Never,
}

impl PrivacyExceptionKind {
    fn label(self) -> &'static str {
        match self {
            PrivacyExceptionKind::Always => "Always allow",
            PrivacyExceptionKind::Never => "Never allow",
        }
    }
}
impl QuillApp {
    /// Slice S3: open the Privacy overlay (TGX Settings → Privacy).
    /// Live: fetch all five privacy rules, the read-date setting, and the
    /// first blocked page (unguarded: always fresh on open). Demo: the
    /// `ReadyPrivacy` fixture seeds state directly.
    pub(crate) fn open_privacy(&mut self, cx: &mut Context<Self>) {
        self.privacy_open = true;
        self.privacy_editor = None;
        self.privacy_exceptions = None;
        self.exception_picker_open = false;
        self.block_picker_open = false;
        self.unblock_confirm = None;
        if let Some(live) = self.live.as_mut() {
            for key in PrivacySettingKey::all() {
                let _ = live.driver.fetch_privacy_rules(key);
            }
            let _ = live.driver.fetch_read_date_privacy();
            let _ = live.driver.fetch_blocked_senders();
            let _ = live.driver.fetch_new_chat_privacy();
            if let Some(me) = live.driver.session.my_user_id {
                let _ = live.driver.fetch_user_full_info(me);
            }
        }
        cx.notify();
    }

    /// Slice S3: close the topmost privacy layer (exceptions →
    /// exceptions-picker → editor → main overlay), like TGX's back stack.
    #[allow(clippy::if_same_then_else)]
    pub(super) fn close_privacy_top(&mut self, cx: &mut Context<Self>) {
        if self.exception_picker_open || self.block_picker_open {
            self.exception_picker_open = false;
            self.block_picker_open = false;
        } else if self.privacy_exceptions.take().is_some() {
        } else if self.privacy_editor.take().is_some() {
        } else {
            self.privacy_open = false;
        }
        self.unblock_confirm = None;
        cx.notify();
    }

    /// Slice S3: shared dialog shell for the privacy overlays (mirrors
    /// `storage_usage_overlay`): dim backdrop + centered card.
    pub(super) fn privacy_shell(
        &self,
        cx: &mut Context<Self>,
        id: &str,
        title: &'static str,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .id(format!("privacy-overlay-{id}"))
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id(format!("privacy-backdrop-{id}"))
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_privacy_top(cx);
                    })),
            )
            .child(
                div()
                    .id(format!("privacy-dialog-{id}"))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(cx.theme().sidebar)
                    .border_1()
                    .border_color(cx.theme().border)
                    .w(px(520.))
                    .max_w(relative(0.9))
                    .max_h(relative(0.85))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_semibold().child(title))
                            .child(
                                Button::new(format!("privacy-close-{id}"))
                                    .icon(gpui_kit::assets::IconName::X)
                                    .tooltip("Close")
                                    .accessibility_label("Close")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_privacy_top(cx);
                                    })),
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    /// Slice S3: the main Privacy overlay — the five visibility rules,
    /// the two call rows, and the blocked-users list (TGX
    /// `SettingsPrivacyController`).
    pub(crate) fn privacy_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_3();

        if self
            .session()
            .is_some_and(|s| s.privacy_data.check_password_suggested)
        {
            body = body.child(self.password_check_card(cx));
        }

        let mut visibility = div().flex().flex_col().gap_1();
        visibility = visibility.child(div().text_sm().font_semibold().px_1().child("Who can see"));
        for key in PrivacySettingKey::visibility_rows() {
            visibility = visibility.child(self.privacy_rule_row(cx, key));
        }
        body = body.child(visibility);

        let mut contact = div().flex().flex_col().gap_1();
        contact = contact.child(
            div()
                .text_sm()
                .font_semibold()
                .px_1()
                .child("Who can contact me"),
        );
        contact = contact.child(self.privacy_call_row(
            cx,
            "allow",
            "Who can call me",
            None,
            PrivacyEditorTarget::CallAllow,
        ));
        contact = contact.child(self.privacy_call_row(
            cx,
            "p2p",
            "Peer-to-peer calls",
            Some("Use peer-to-peer for voice and video calls when possible"),
            PrivacyEditorTarget::CallP2P,
        ));
        contact = contact.child(self.privacy_rule_row(cx, PrivacySettingKey::AllowVoiceMessages));
        contact = contact.child(self.new_chat_privacy_row(cx));
        contact = contact.child(self.privacy_rule_row(cx, PrivacySettingKey::AllowChatInvites));
        body = body.child(contact);

        body = body.child(self.privacy_frequent_contacts_section(cx));

        body = body.child(self.privacy_blocked_section(cx));

        body = body.child(self.privacy_security_section(cx));

        // Slice payments: the "Clear saved payment/shipping info" row
        // (`parity:bots-payment-clear`) — destructive, with the shared
        // confirm dialog, next to the other data-clearing controls.
        let mut payments = div().flex().flex_col().gap_1();
        payments = payments.child(div().text_sm().font_semibold().px_1().child("Payments"));
        payments = payments.child(
            div().flex().flex_col().gap_1().child(
                Button::new("privacy-clear-payment-info")
                    .small()
                    .label("Clear saved payment/shipping info…")
                    .custom(super::security::quiet_danger(cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_group_confirm(
                            ChatId(0),
                            GroupConfirmAction::ClearPaymentInfo,
                            cx,
                        );
                    })),
            ),
        );
        payments = payments.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Deletes the shipping info and payment credentials Telegram saved from past checkouts."),
        );
        body = body.child(payments);

        self.privacy_shell(cx, "main", "Privacy", body.into_any_element())
    }

    /// tdesktop Settings > Privacy > "Suggest frequent contacts": whether
    /// the search panel shows the people you message most
    /// (`lng_settings_top_peers_*`; TDLib option `disable_top_chats`).
    fn privacy_frequent_contacts_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = !self.session().is_some_and(|s| s.search.top_chats_disabled);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child("Frequent contacts"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .child(div().text_sm().child("Suggest frequent contacts"))
                    .child(
                        Switch::new("privacy-suggest-frequent-contacts")
                            .checked(enabled)
                            .accessibility_label("Suggest frequent contacts")
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                this.set_top_chats_disabled(!on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Display people you message frequently at the top of the search section for quick access.",
                    ),
            )
            .into_any_element()
    }

    /// Slice S3: one visibility-rule row (label + current value).
    fn privacy_rule_row(&self, cx: &mut Context<Self>, key: PrivacySettingKey) -> AnyElement {
        let value = self
            .session()
            .map(|s| privacy_key_value(s, key))
            .unwrap_or_default();
        let target = PrivacyEditorTarget::Rule(key);
        div()
            .id(format!("privacy-rule-{}", key.td_type()))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{} · {}", target.label(), value))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_sm().child(key.label()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(value),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.privacy_editor = Some(target);
                this.exception_picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Slice S3: one call-privacy row (reuses the Phase C2i state the
    /// Calls tab edits).
    fn privacy_call_row(
        &self,
        cx: &mut Context<Self>,
        id: &str,
        label: &'static str,
        subtitle: Option<&'static str>,
        target: PrivacyEditorTarget,
    ) -> AnyElement {
        let session = self.session();
        let (who, loading, error) = match target {
            PrivacyEditorTarget::CallAllow => (
                session.and_then(|s| s.call_privacy_allow_calls),
                session.is_some_and(|s| s.call_privacy_loading),
                session.is_some_and(|s| s.call_privacy_error),
            ),
            PrivacyEditorTarget::CallP2P => (
                session.and_then(|s| s.call_privacy_p2p),
                session.is_some_and(|s| s.call_privacy_loading),
                session.is_some_and(|s| s.call_privacy_error),
            ),
            PrivacyEditorTarget::Rule(_)
            | PrivacyEditorTarget::NewChat
            | PrivacyEditorTarget::FileOpen => (None, false, false),
        };
        let value = if loading {
            "Loading…".to_string()
        } else if error {
            "Couldn't load".to_string()
        } else {
            who.map(|w| w.label().to_string())
                .unwrap_or_else(|| "Custom".to_string())
        };
        let mut text = div().flex().flex_col().flex_1().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(value),
                ),
        );
        if let Some(subtitle) = subtitle {
            text = text.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(subtitle),
            );
        }
        div()
            .id(format!("privacy-calls-{id}"))
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .child(text)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.privacy_editor = Some(target);
                this.exception_picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Slice S3: the blocked-users section (TGX
    /// `SettingsBlockedController`): list with two-step Unblock,
    /// "Block…" contact picker, and paging.
    fn privacy_blocked_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let total = session.map(|s| s.blocked_total).unwrap_or(0);
        let list: Vec<i64> = session
            .and_then(|s| s.blocked_senders.clone())
            .unwrap_or_default();
        let loading = session.is_some_and(|s| s.blocked_loading);
        let error = session.is_some_and(|s| s.blocked_error);

        let mut section = div().flex().flex_col().gap_1();
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().font_semibold().child("Blocked users"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{total}")),
                        ),
                )
                .child(
                    Button::new("privacy-block-add")
                        .small()
                        .label("Block…")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.block_picker_open = true;
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.fetch_contacts();
                            }
                            cx.notify();
                        })),
                ),
        );
        if error && list.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("Couldn't load blocked users."),
            );
        } else if loading && list.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("Loading…"),
            );
        } else if list.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("No blocked users."),
            );
        }
        for user_id in &list {
            section = section.child(self.blocked_user_row(cx, *user_id));
        }
        if (total as usize) > list.len() {
            let remaining = total as usize - list.len();
            section = section.child(
                Button::new("privacy-blocked-more")
                    .small()
                    .label(format!("Load more ({remaining} remaining)"))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.fetch_blocked_senders();
                        }
                        cx.notify();
                    })),
            );
        }
        if self.block_picker_open {
            section = section.child(self.block_picker(cx, &list));
        }
        section.into_any_element()
    }

    /// Slice S3: one blocked user with two-step Unblock (TGX
    /// `SettingsBlockedController` confirms before unblocking).
    fn blocked_user_row(&self, cx: &mut Context<Self>, user_id: i64) -> AnyElement {
        let name = self
            .session()
            .and_then(|s| s.users.get(&user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let confirming = self.unblock_confirm == Some(user_id);
        let mut row = div()
            .id(format!("privacy-blocked-{user_id}"))
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .child(div().text_sm().child(name));
        if confirming {
            row = row.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(format!("privacy-unblock-confirm-{user_id}"))
                            .small()
                            .label("Unblock")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.unblock_user(user_id, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("privacy-unblock-cancel-{user_id}"))
                            .small()
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unblock_confirm = None;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            row = row.child(
                Button::new(format!("privacy-unblock-{user_id}"))
                    .small()
                    .label("Unblock")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.unblock_confirm = Some(user_id);
                        cx.notify();
                    })),
            );
        }
        row.into_any_element()
    }

    /// Slice S3: block picker — contacts not already blocked.
    fn block_picker(&self, cx: &mut Context<Self>, blocked: &[i64]) -> AnyElement {
        let session = self.session();
        let rows: Vec<ContactRow> = session.map(|s| s.contact_rows()).unwrap_or_default();
        let rows: Vec<&ContactRow> = rows
            .iter()
            .filter(|r| !blocked.contains(&r.user_id))
            .collect();
        let mut body = div().flex().flex_col().gap_1().mt_1();
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .px_1()
                .child("Choose a contact to block:"),
        );
        if rows.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("No more contacts."),
            );
        }
        for row in rows {
            let user_id = row.user_id;
            body = body.child(
                div()
                    .id(format!("privacy-block-pick-{user_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Block {}", row.name))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().child(row.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.block_user(user_id, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// Slice S3: block a user (`setMessageSenderBlockList` with
    /// `blockListMain`). Optimistic; the `ok`/error confirms or fails it.
    fn block_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        self.block_picker_open = false;
        if self.live.is_some() {
            let result = {
                let live = self.live.as_mut().expect("live checked above");
                live.driver.block_sender(user_id)
            };
            if let Err(err) = result {
                self.status_note = format!("block failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            let list = demo.blocked_senders.get_or_insert_with(Vec::new);
            if !list.contains(&user_id) {
                list.push(user_id);
                demo.blocked_total += 1;
            }
        }
        cx.notify();
    }

    /// Slice S3: unblock a user (`setMessageSenderBlockList` with a null
    /// block list — TGX `Tdlib.unblockSender`). Optimistic.
    fn unblock_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        self.unblock_confirm = None;
        if self.live.is_some() {
            let result = {
                let live = self.live.as_mut().expect("live checked above");
                live.driver.unblock_sender(user_id)
            };
            if let Err(err) = result {
                self.status_note = format!("unblock failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut()
            && let Some(list) = demo.blocked_senders.as_mut()
        {
            list.retain(|id| *id != user_id);
            demo.blocked_total = demo.blocked_total.saturating_sub(1);
        }
        cx.notify();
    }

    /// Slice S3 + B13: per-rule editor overlay (tdesktop
    /// `EditPrivacyBox`): the key's radios (Everybody / My contacts /
    /// Nobody; "find me by number" has only the first two), always/never
    /// exception rows with the Premium / Mini Apps rows where tdesktop
    /// offers them, the "Hide read time" toggle for Last Seen, the "find
    /// me by my number" choice under a hidden phone number, and the gift
    /// settings under Gifts.
    pub(crate) fn privacy_editor_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.privacy_editor?;
        match target {
            PrivacyEditorTarget::NewChat => return Some(self.new_chat_editor(cx)),
            PrivacyEditorTarget::FileOpen => return Some(self.file_open_editor(cx)),
            _ => {}
        }
        let session = self.session();
        let current: Option<PrivacyWho> =
            match target {
                PrivacyEditorTarget::Rule(key) => session
                    .and_then(|s| s.privacy.get(&key))
                    .and_then(|st| match st {
                        PrivacyKeyState::Ready(d) => d.who,
                        _ => None,
                    }),
                PrivacyEditorTarget::CallAllow => session.and_then(|s| s.call_privacy_allow_calls),
                PrivacyEditorTarget::CallP2P => session.and_then(|s| s.call_privacy_p2p),
                PrivacyEditorTarget::NewChat | PrivacyEditorTarget::FileOpen => None,
            };
        let mut body = div().flex().flex_col().gap_1();
        if let PrivacyEditorTarget::Rule(key) = target {
            body = body.child(
                div()
                    .text_xs()
                    .font_semibold()
                    .px_1()
                    .text_color(cx.theme().muted_foreground)
                    .child(key.header()),
            );
        }
        let options: &[PrivacyWho] = match target {
            PrivacyEditorTarget::Rule(key) => key.options(),
            _ => &[
                PrivacyWho::Everybody,
                PrivacyWho::Contacts,
                PrivacyWho::Nobody,
            ],
        };
        for &who in options {
            body = body.child(self.privacy_radio_row(cx, target, who, current));
        }
        if let PrivacyEditorTarget::Rule(key) = target {
            // tdesktop `PhoneNumberPrivacyController::setupMiddleWidget`:
            // the find-by-number choice shows while the number is hidden
            // from everybody.
            if key == PrivacySettingKey::ShowPhoneNumber && current == Some(PrivacyWho::Nobody) {
                body = body.child(self.find_by_number_block(cx));
            }
            if key == PrivacySettingKey::AutosaveGifts {
                body = body.child(self.gift_settings_block(cx));
            }
            if key.has_exceptions() {
                let detail = session
                    .and_then(|s| s.privacy.get(&key))
                    .and_then(|st| match st {
                        PrivacyKeyState::Ready(d) => Some(d.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let (always, never) = detail.exception_counts();
                body = body.child(self.privacy_exception_row(
                    cx,
                    target,
                    PrivacyExceptionKind::Always,
                    always,
                ));
                body = body.child(self.privacy_exception_row(
                    cx,
                    target,
                    PrivacyExceptionKind::Never,
                    never,
                ));
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(cx.theme().muted_foreground)
                        .child(key.exceptions_note()),
                );
                // TGX `SettingsPrivacyKeyController.needExtraToggle`: the
                // "Hide read time" toggle shows unless the mode is Everybody
                // with no never-exceptions.
                if key == PrivacySettingKey::ShowStatus
                    && (current != Some(PrivacyWho::Everybody) || !detail.never.is_empty())
                {
                    body = body.child(self.read_date_toggle_row(cx));
                }
            }
            if key.restriction_needs_premium()
                && !session.is_some_and(|s| s.premium_option == Some(true))
            {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Restricting who can send you voice messages needs Telegram Premium.",
                        ),
                );
            }
        }
        Some(self.privacy_shell(cx, "editor", target.label(), body.into_any_element()))
    }

    /// Slice S3: one Everybody / My contacts / Nobody radio row.
    pub(super) fn privacy_radio_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        who: PrivacyWho,
        current: Option<PrivacyWho>,
    ) -> AnyElement {
        let selected = current == Some(who);
        div()
            .id(format!(
                "privacy-choice-{}-{}",
                target.label().replace(' ', "-"),
                who.label().replace(' ', "-")
            ))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{} · {}", target.label(), who.label()))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(if selected {
                cx.theme().accent.opacity(0.15)
            } else {
                cx.theme().background
            })
            .child(div().text_xs().child(if selected { "●" } else { "○" }))
            .child(div().text_sm().child(who.label()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_privacy_target_who(target, who, cx);
            }))
            .into_any_element()
    }

    /// Slice S3: change one rule's base choice. Live: optimistic `set`
    /// that keeps the current exception rules (TGX `toggleGlobal`).
    /// Demo: mutate the fixture directly.
    pub(super) fn set_privacy_target_who(
        &mut self,
        target: PrivacyEditorTarget,
        who: PrivacyWho,
        cx: &mut Context<Self>,
    ) {
        // tdesktop `VoicesPrivacyController::premiumClickedCallback`:
        // narrowing who can send voice messages needs Premium.
        if let PrivacyEditorTarget::Rule(key) = target
            && key.restriction_needs_premium()
            && who != PrivacyWho::Everybody
            && !self
                .session()
                .is_some_and(|s| s.premium_option == Some(true))
        {
            self.status_note =
                "Restricting who can send you voice messages needs Telegram Premium.".into();
            cx.notify();
            return;
        }
        if self.live.is_some() {
            // Scoped borrow: the driver result is owned, so the
            // `status_note` write below doesn't alias the live borrow.
            let result = {
                let live = self.live.as_mut().expect("live checked above");
                match target {
                    PrivacyEditorTarget::Rule(key) => {
                        // Not Ready (Loading/Failed): ignore the click — sending
                        // a base-only detail here would wipe the server exceptions.
                        let detail = match live.driver.session.privacy.get(&key) {
                            Some(PrivacyKeyState::Ready(d)) => d.with_base(who),
                            _ => return,
                        };
                        live.driver.set_privacy_rules(key, detail)
                    }
                    PrivacyEditorTarget::CallAllow => live
                        .driver
                        .set_call_privacy(CallPrivacySetting::AllowCalls, who),
                    PrivacyEditorTarget::CallP2P => live
                        .driver
                        .set_call_privacy(CallPrivacySetting::PeerToPeer, who),
                    PrivacyEditorTarget::NewChat | PrivacyEditorTarget::FileOpen => return,
                }
            };
            if let Err(err) = result {
                self.status_note = format!("privacy update failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            match target {
                PrivacyEditorTarget::Rule(key) => {
                    // Not Ready (Loading/Failed): ignore the click — inserting
                    // a base-only detail here would wipe the server exceptions.
                    let detail = match demo.privacy.get(&key) {
                        Some(PrivacyKeyState::Ready(d)) => d.with_base(who),
                        _ => return,
                    };
                    demo.privacy.insert(key, PrivacyKeyState::Ready(detail));
                }
                PrivacyEditorTarget::CallAllow => demo.call_privacy_allow_calls = Some(who),
                PrivacyEditorTarget::CallP2P => demo.call_privacy_p2p = Some(who),
                PrivacyEditorTarget::NewChat | PrivacyEditorTarget::FileOpen => return,
            }
        }
        cx.notify();
    }

    /// Slice S3: one always/never exception row with its user count.
    fn privacy_exception_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        count: usize,
    ) -> AnyElement {
        div()
            .id(format!("privacy-exceptions-{kind:?}"))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{} · {} exceptions", target.label(), kind.label()))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_sm().child(kind.label()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if count == 0 {
                        "None".to_string()
                    } else {
                        format!("{count}")
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.privacy_exceptions = Some((target, kind));
                this.exception_picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Slice S3: "Hide read time" toggle (TGX
    /// `SettingsPrivacyKeyController` extra toggle under Last Seen).
    /// Checked = read dates hidden (`!show_read_date`).
    fn read_date_toggle_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let hidden = !self
            .session()
            .and_then(|s| s.read_date_show)
            .unwrap_or(true);
        let loading = self.session().is_some_and(|s| s.read_date_loading);
        div()
            .id("privacy-hide-read-time")
            .role(gpui_kit::Role::Button)
            .aria_label("Hide read time")
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_xs().child(if hidden { "☑" } else { "☐" }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child("Hide read time"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loading {
                                "Updating…"
                            } else {
                                "Don't share when you've read messages"
                            }),
                    ),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_read_date(cx);
            }))
            .into_any_element()
    }

    /// Slice S3: flip `show_read_date`. Live: optimistic `set`; demo:
    /// flip the fixture.
    fn toggle_read_date(&mut self, cx: &mut Context<Self>) {
        let next = !self
            .session()
            .and_then(|s| s.read_date_show)
            .unwrap_or(true);
        if self.live.is_some() {
            let result = {
                let live = self.live.as_mut().expect("live checked above");
                live.driver.set_read_date_privacy(next)
            };
            if let Err(err) = result {
                self.status_note = format!("read-date update failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.read_date_show = Some(next);
        }
        cx.notify();
    }

    /// Slice S3 + B13: the always/never exception list (tdesktop
    /// `PrivacyExceptionsBoxController`): the Premium users / Mini Apps
    /// rows where tdesktop offers them, then users and groups with Remove
    /// and Add pickers.
    pub(crate) fn privacy_exceptions_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (target, kind) = self.privacy_exceptions?;
        let session = self.session()?;
        let PrivacyEditorTarget::Rule(key) = target else {
            return None;
        };
        let detail = session
            .privacy
            .get(&key)
            .and_then(|st| match st {
                PrivacyKeyState::Ready(d) => Some(d.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let (ids, chat_ids): (&[i64], &[i64]) = match kind {
            PrivacyExceptionKind::Always => (&detail.always, &detail.always_chats),
            PrivacyExceptionKind::Never => (&detail.never, &detail.never_chats),
        };
        let mut body = div().flex().flex_col().gap_1();
        if key.allows_premium_exception(kind == PrivacyExceptionKind::Always) {
            body = body.child(self.exception_flag_row(
                cx,
                target,
                kind,
                ExceptionFlag::Premium,
                "Premium users",
                "all Telegram Premium subscribers",
                detail.allow_premium,
            ));
        }
        if key.allows_bots_exception() {
            let on = match kind {
                PrivacyExceptionKind::Always => detail.allow_bots,
                PrivacyExceptionKind::Never => detail.never_bots,
            };
            body = body.child(self.exception_flag_row(
                cx,
                target,
                kind,
                ExceptionFlag::Bots,
                "Mini Apps",
                "web mini apps that you use",
                on,
            ));
        }
        if ids.is_empty() && chat_ids.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("No users or groups added yet."),
            );
        }
        for user_id in ids {
            let user_id = *user_id;
            let name = session
                .users
                .get(&user_id)
                .map(|u| u.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            body = body.child(
                div()
                    .id(format!("privacy-exception-user-{user_id}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .child(div().text_sm().child(name))
                    .child(
                        Button::new(format!("privacy-exception-remove-{user_id}"))
                            .small()
                            .label("Remove")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_exception(target, kind, user_id, false, cx);
                            })),
                    ),
            );
        }
        for chat_id in chat_ids {
            let chat_id = *chat_id;
            let title = session
                .chats
                .get(&chat_id)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| format!("Group {chat_id}"));
            body = body.child(
                div()
                    .id(format!("privacy-exception-chat-{chat_id}"))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .child(div().text_sm().child(title))
                    .child(
                        Button::new(format!("privacy-exception-remove-chat-{chat_id}"))
                            .small()
                            .label("Remove")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_exception_chat(target, kind, chat_id, false, cx);
                            })),
                    ),
            );
        }
        body = body.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("privacy-exception-add")
                        .small()
                        .label("Add user…")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.exception_picker_open = true;
                            this.privacy_ui.exception_picker_groups = false;
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.fetch_contacts();
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("privacy-exception-add-group")
                        .small()
                        .label("Add group…")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.exception_picker_open = true;
                            this.privacy_ui.exception_picker_groups = true;
                            cx.notify();
                        })),
                ),
        );
        if self.exception_picker_open {
            body = if self.privacy_ui.exception_picker_groups {
                body.child(self.exception_group_picker(cx, target, kind, &detail))
            } else {
                body.child(self.exception_picker(cx, target, kind, &detail))
            };
        }
        Some(self.privacy_shell(cx, "exceptions", kind.label(), body.into_any_element()))
    }

    /// One Premium users / Mini Apps switch row of an exception list.
    #[allow(clippy::too_many_arguments)]
    fn exception_flag_row(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        flag: ExceptionFlag,
        label: &'static str,
        about: &'static str,
        on: bool,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(about),
                    ),
            )
            .child(
                Switch::new(format!("privacy-exception-flag-{flag:?}-{kind:?}"))
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                        this.edit_exception_flag(target, kind, flag, on, cx);
                    })),
            )
            .into_any_element()
    }

    /// Slice S3: add-exception contact picker — contacts not already in
    /// either exception list.
    fn exception_picker(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        detail: &PrivacyRuleDetail,
    ) -> AnyElement {
        let session = self.session();
        let rows: Vec<ContactRow> = session.map(|s| s.contact_rows()).unwrap_or_default();
        let rows: Vec<&ContactRow> = rows
            .iter()
            .filter(|r| !detail.always.contains(&r.user_id) && !detail.never.contains(&r.user_id))
            .collect();
        let mut body = div().flex().flex_col().gap_1().mt_1();
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .px_1()
                .child("Choose a contact:"),
        );
        if rows.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("No more contacts."),
            );
        }
        for row in rows {
            let user_id = row.user_id;
            body = body.child(
                div()
                    .id(format!("privacy-picker-user-{user_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Add {} to privacy exceptions", row.name))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().child(row.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_exception(target, kind, user_id, true, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// B13: add-exception group picker — the user's groups and
    /// supergroups (tdesktop's exception lists take chats whose members
    /// the rule then covers), not already in either list.
    fn exception_group_picker(
        &self,
        cx: &mut Context<Self>,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        detail: &PrivacyRuleDetail,
    ) -> AnyElement {
        let groups: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.ordered_chats()
                    .into_iter()
                    .filter(|chat| {
                        matches!(
                            chat.kind,
                            quill::telegram::envelope::ChatKind::BasicGroup { .. }
                                | quill::telegram::envelope::ChatKind::Supergroup {
                                    is_channel: false,
                                    ..
                                }
                        )
                    })
                    .filter(|chat| {
                        !detail.always_chats.contains(&chat.id.0)
                            && !detail.never_chats.contains(&chat.id.0)
                    })
                    .map(|chat| (chat.id.0, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut body = div().flex().flex_col().gap_1().mt_1();
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .px_1()
                .child("Choose a group:"),
        );
        if groups.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child("No more groups."),
            );
        }
        for (chat_id, title) in groups {
            body = body.child(
                div()
                    .id(format!("privacy-picker-chat-{chat_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Add {title} to privacy exceptions"))
                    .tab_index(0)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_exception_chat(target, kind, chat_id, true, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// Apply `edit` to the key's rule detail and send/store the result.
    /// Not Ready (Loading/Failed): ignored — an empty detail would
    /// recompose into a base-less rule list (first-match => deny-all).
    fn mutate_privacy_detail(
        &mut self,
        key: PrivacySettingKey,
        edit: impl FnOnce(&mut PrivacyRuleDetail),
        cx: &mut Context<Self>,
    ) {
        let current = self.session().and_then(|s| {
            s.privacy.get(&key).and_then(|st| match st {
                PrivacyKeyState::Ready(d) => Some(d.clone()),
                _ => None,
            })
        });
        let Some(mut detail) = current else {
            return;
        };
        edit(&mut detail);
        if self.live.is_some() {
            let result = {
                let live = self.live.as_mut().expect("live checked above");
                live.driver.set_privacy_rules(key, detail)
            };
            if let Err(err) = result {
                self.status_note = format!("privacy update failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.privacy.insert(key, PrivacyKeyState::Ready(detail));
        }
        cx.notify();
    }

    /// Slice S3: add or remove one user from an always/never exception
    /// list. Adding to one list removes the user from the other (a user
    /// can't meaningfully be in both under first-match evaluation).
    fn edit_exception(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        user_id: i64,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| {
                let (mine, other) = match kind {
                    PrivacyExceptionKind::Always => (&mut detail.always, &mut detail.never),
                    PrivacyExceptionKind::Never => (&mut detail.never, &mut detail.always),
                };
                if add {
                    other.retain(|id| *id != user_id);
                    if !mine.contains(&user_id) {
                        mine.push(user_id);
                    }
                } else {
                    mine.retain(|id| *id != user_id);
                }
            },
            cx,
        );
        if add {
            self.exception_picker_open = false;
        }
        cx.notify();
    }

    /// B13: add or remove one group (`...ChatMembers` rules) of an
    /// always/never list; the other list drops it, like users.
    pub(super) fn edit_exception_chat(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        chat_id: i64,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| {
                let (mine, other) = match kind {
                    PrivacyExceptionKind::Always => {
                        (&mut detail.always_chats, &mut detail.never_chats)
                    }
                    PrivacyExceptionKind::Never => {
                        (&mut detail.never_chats, &mut detail.always_chats)
                    }
                };
                if add {
                    other.retain(|id| *id != chat_id);
                    if !mine.contains(&chat_id) {
                        mine.push(chat_id);
                    }
                } else {
                    mine.retain(|id| *id != chat_id);
                }
            },
            cx,
        );
        if add {
            self.exception_picker_open = false;
        }
        cx.notify();
    }

    /// B13: flip the Premium users / Mini Apps row of an exception list.
    /// Choosing a row in one list clears it in the other (tdesktop
    /// `EditPrivacyBox::editExceptions`).
    pub(super) fn edit_exception_flag(
        &mut self,
        target: PrivacyEditorTarget,
        kind: PrivacyExceptionKind,
        flag: ExceptionFlag,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let PrivacyEditorTarget::Rule(key) = target else {
            return;
        };
        self.mutate_privacy_detail(
            key,
            |detail| match (flag, kind) {
                (ExceptionFlag::Premium, _) => detail.allow_premium = on,
                (ExceptionFlag::Bots, PrivacyExceptionKind::Always) => {
                    detail.allow_bots = on;
                    if on {
                        detail.never_bots = false;
                    }
                }
                (ExceptionFlag::Bots, PrivacyExceptionKind::Never) => {
                    detail.never_bots = on;
                    if on {
                        detail.allow_bots = false;
                    }
                }
            },
            cx,
        );
    }
}

/// B13: the non-peer rows of an exception list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExceptionFlag {
    /// `userPrivacySettingRuleAllowPremiumUsers`.
    Premium,
    /// `userPrivacySettingRuleAllowBots` / `...RestrictBots`.
    Bots,
}

/// Slice S3: `ReadyPrivacy` fixture — the five privacy rules (with
/// always/never exceptions), the read-date setting, one blocked user,
/// and contacts for the pickers (injected, no live Telegram). Rule
/// state is seeded directly like `demo_storage_stats`; the reducer path
/// for fetched rules is covered by `state.rs` unit tests.
pub(crate) fn apply_ready_privacy(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in [
        r#"{"@type":"updateUser","user":{"id":61,"first_name":"Maya","last_name":"Chen","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":62,"first_name":"Leo","last_name":"Park","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":63,"first_name":"Ana","last_name":"Ruiz","type":{"@type":"userTypeRegular"}}}"#,
    ] {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let mut ready = |key: PrivacySettingKey, who: PrivacyWho, always: &[i64], never: &[i64]| {
        session.privacy.insert(
            key,
            PrivacyKeyState::Ready(PrivacyRuleDetail {
                who: Some(who),
                always: always.to_vec(),
                never: never.to_vec(),
                ..Default::default()
            }),
        );
    };
    ready(
        PrivacySettingKey::ShowStatus,
        PrivacyWho::Contacts,
        &[61],
        &[62],
    );
    ready(
        PrivacySettingKey::ShowPhoneNumber,
        PrivacyWho::Nobody,
        &[],
        &[],
    );
    ready(
        PrivacySettingKey::ShowProfilePhoto,
        PrivacyWho::Everybody,
        &[],
        &[],
    );
    ready(
        PrivacySettingKey::ShowLinkInForwardedMessages,
        PrivacyWho::Contacts,
        &[],
        &[63],
    );
    ready(
        PrivacySettingKey::AllowChatInvites,
        PrivacyWho::Nobody,
        &[61],
        &[],
    );
    ready(PrivacySettingKey::ShowBio, PrivacyWho::Contacts, &[], &[]);
    ready(
        PrivacySettingKey::ShowBirthdate,
        PrivacyWho::Contacts,
        &[61],
        &[],
    );
    ready(
        PrivacySettingKey::ShowProfileAudio,
        PrivacyWho::Everybody,
        &[],
        &[62],
    );
    ready(
        PrivacySettingKey::AllowFindingByPhoneNumber,
        PrivacyWho::Contacts,
        &[],
        &[],
    );
    ready(
        PrivacySettingKey::AllowVoiceMessages,
        PrivacyWho::Everybody,
        &[],
        &[],
    );
    ready(
        PrivacySettingKey::AutosaveGifts,
        PrivacyWho::Everybody,
        &[],
        &[],
    );
    // tdesktop offers Premium users in the Always list of "Groups &
    // Channels" and Mini Apps in both lists of "Gifts".
    if let Some(PrivacyKeyState::Ready(detail)) = session
        .privacy
        .get_mut(&PrivacySettingKey::AllowChatInvites)
    {
        detail.allow_premium = true;
        detail.always_chats = vec![9001];
    }
    if let Some(PrivacyKeyState::Ready(detail)) =
        session.privacy.get_mut(&PrivacySettingKey::AutosaveGifts)
    {
        detail.never_bots = true;
    }
    session.my_user_id = Some(60);
    session.premium_option = Some(true);
    session.user_full_infos.insert(
        60,
        quill::state::UserFullInfoData {
            extras: quill::telegram::envelope::UserProfileExtras {
                gift_settings: Some(quill::privacy::GiftSettings {
                    show_gift_button: true,
                    limited_gifts: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    session.privacy_data.new_chat = Some(quill::privacy::NewChatPrivacyState::Ready(
        quill::privacy::NewChatPrivacy {
            allow_from_unknown: false,
            incoming_paid_message_star_count: 0,
        },
    ));
    session.privacy_data.can_ignore_sensitive = true;
    session.privacy_data.ignore_sensitive = Some(false);
    session.privacy_data.check_password_suggested = true;
    session.privacy_data.inactive_session_ttl_days = Some(180);
    session.read_date_show = Some(true);
    session.call_privacy_allow_calls = Some(PrivacyWho::Contacts);
    session.call_privacy_p2p = Some(PrivacyWho::Everybody);
    session.blocked_senders = Some(vec![63]);
    session.blocked_total = 1;
    session.contacts = Some(vec![61, 62, 63]);
}
/// Slice S3: the Privacy-screen row value for one rule key: the base
/// choice, with the always/never exception counts when non-zero (TGX
/// `SettingsPrivacyKeyController` shows the mode plus exceptions).
fn privacy_key_value(session: &Session, key: PrivacySettingKey) -> String {
    match session.privacy.get(&key) {
        None | Some(PrivacyKeyState::Loading) => "Loading…".to_string(),
        Some(PrivacyKeyState::Failed) => "Couldn't load".to_string(),
        Some(PrivacyKeyState::Ready(detail)) => {
            let mut value = detail
                .who
                .map(|w| w.label().to_string())
                .unwrap_or_else(|| "Custom".to_string());
            let (always, never) = detail.exception_counts();
            if always > 0 || never > 0 {
                value.push_str(&format!(" (+{always}/−{never})"));
            }
            value
        }
    }
}

/// B13: the "Who can message me" row value (tdesktop Messages privacy).
pub(super) fn new_chat_privacy_value(session: &Session) -> String {
    match session.privacy_data.new_chat {
        None | Some(quill::privacy::NewChatPrivacyState::Loading) => "Loading…".to_string(),
        Some(quill::privacy::NewChatPrivacyState::Failed) => "Couldn't load".to_string(),
        Some(quill::privacy::NewChatPrivacyState::Ready(settings)) => {
            if settings.incoming_paid_message_star_count > 0 {
                format!(
                    "{} Stars per message",
                    settings.incoming_paid_message_star_count
                )
            } else if settings.allow_from_unknown {
                "Everybody".to_string()
            } else {
                "Contacts & Premium".to_string()
            }
        }
    }
}

//! Settings → Privacy slice (TGX `SettingsPrivacyController`): the
//! privacy overlay, per-rule editor, always/never exception lists, the
//! read-date toggle, and the blocked-users section. Server state flows
//! through TDLib `getUserPrivacySettingRules` / `setUserPrivacySettingRules`
//! (+ read-date and blocked-sender constructors); unknown rule types
//! round-trip untouched via `PrivacyRuleDetail::extra_rules`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use crate::ui::scroll_load::{load_due, loading_row, scrolled_near_end, watch};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::state::{ContactRow, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The tallest the blocked-users rows grow before they scroll.
const BLOCKED_ROWS_HEIGHT: f32 = 360.;

/// Slice S3: Privacy rule editor target — one of the five
/// `PrivacySettingKey` rules, or one of the two call settings (which
/// reuse the Phase C2i `call_privacy_*` plumbing instead of the new
/// per-key state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrivacyEditorTarget {
    Rule(PrivacySettingKey),
    /// B13: "Who can message me" (`newChatPrivacySettings`).
    NewChat,
    /// B13: "File open confirmations" (extension whitelist + IP warning).
    FileOpen,
}

impl PrivacyEditorTarget {
    fn label(self) -> &'static str {
        match self {
            PrivacyEditorTarget::Rule(key) => key.label(),
            PrivacyEditorTarget::NewChat => tr(keys::Q_PREFS_WHO_CAN_MESSAGE_ME),
            PrivacyEditorTarget::FileOpen => tr(keys::Q_PREFS_FILE_OPEN_CONFIRMATIONS),
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
            PrivacyExceptionKind::Always => tr(keys::EDIT_PRIVACY_GROUPS_ALWAYS_EMPTY),
            PrivacyExceptionKind::Never => tr(keys::EDIT_PRIVACY_GROUPS_NEVER_EMPTY),
        }
    }
}
impl QuillApp {
    /// Settings → Privacy, with nothing open over it, freshly fetched.
    pub(crate) fn open_privacy(&mut self, cx: &mut Context<Self>) {
        self.privacy.editor = None;
        self.privacy.exceptions = None;
        self.privacy.exception_picker_open = false;
        self.privacy.block_picker_open = false;
        self.privacy.unblock_confirm = None;
        self.open_settings(Some(quill::settings_catalog::Page::Privacy));
        self.fetch_privacy();
        cx.notify();
    }

    /// Live: all privacy rules, the read-date setting, the first blocked
    /// page, new-chat and archive settings, unguarded (always fresh). Demo:
    /// the `ReadyPrivacy` fixture seeds state directly.
    pub(crate) fn fetch_privacy(&mut self) {
        if let Some(live) = self.live.as_mut() {
            for key in PrivacySettingKey::all() {
                let _ = live.driver.fetch_privacy_rules(key);
            }
            let _ = live.driver.fetch_read_date_privacy();
            let _ = live.driver.fetch_blocked_senders();
            let _ = live.driver.fetch_new_chat_privacy();
            let _ = live.driver.fetch_archive_chat_list_settings();
            if let Some(me) = live.driver.session.my_user_id {
                let _ = live.driver.fetch_user_full_info(me);
            }
        }
    }

    /// Slice S3: close the topmost privacy layer (exceptions →
    /// exceptions-picker → editor → main overlay), like TGX's back stack.
    #[allow(clippy::if_same_then_else)]
    pub(super) fn close_privacy_top(&mut self, cx: &mut Context<Self>) {
        if self.privacy.exception_picker_open || self.privacy.block_picker_open {
            self.privacy.exception_picker_open = false;
            self.privacy.block_picker_open = false;
        } else if self.privacy.exceptions.take().is_some() {
        } else {
            self.privacy.editor = None;
        }
        self.privacy.unblock_confirm = None;
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
                                    .tooltip(tr(keys::CLOSE))
                                    .accessibility_label(tr(keys::CLOSE))
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

    /// "Clear saved payment/shipping info" (`parity:bots-payment-clear`):
    /// destructive, with the shared confirm dialog.
    pub(crate) fn privacy_payments_section(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child(tr(keys::Q_PREFS_PAYMENTS)),
            )
            .child(
                div().flex().flex_col().gap_1().child(
                    Button::new("privacy-clear-payment-info")
                        .small()
                        .label(tr(keys::Q_PREFS_CLEAR_SAVED_PAYMENT_SHIPPING_INFO))
                        .custom(super::security::quiet_danger(cx))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_group_confirm(
                                ChatId(0),
                                GroupConfirmAction::ClearPaymentInfo,
                                cx,
                            );
                        })),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_DELETES_THE_SHIPPING_INFO_AND)),
            )
            .into_any_element()
    }

    /// tdesktop Settings > Privacy > "Suggest frequent contacts": whether
    /// the search panel shows the people you message most
    /// (`lng_settings_top_peers_*`; TDLib option `disable_top_chats`).
    pub(crate) fn privacy_frequent_contacts_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = !self
            .session()
            .is_some_and(|s| s.search.search.top_chats_disabled);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child(tr(keys::Q_PREFS_FREQUENT_CONTACTS)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_SUGGEST_FREQUENT_CONTACTS)),
                    )
                    .child(
                        Switch::new("privacy-suggest-frequent-contacts")
                            .checked(enabled)
                            .accessibility_label(tr(keys::Q_PREFS_SUGGEST_FREQUENT_CONTACTS))
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
                    .child(tr(keys::SETTINGS_TOP_PEERS_ABOUT)),
            )
            .into_any_element()
    }

    /// Slice S3: the blocked-users section (TGX
    /// `SettingsBlockedController`): list with two-step Unblock,
    /// "Block…" contact picker, and paging.
    pub(crate) fn privacy_blocked_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let total = session.map(|s| s.settings.blocked_total).unwrap_or(0);
        let list: Vec<i64> = session
            .and_then(|s| s.settings.blocked_senders.clone())
            .unwrap_or_default();
        let loading = session.is_some_and(|s| s.settings.blocked_loading);
        let error = session.is_some_and(|s| s.settings.blocked_error);

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
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::Q_PREFS_BLOCKED_USERS)),
                        )
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
                        .label(tr(keys::Q_PREFS_BLOCK))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.privacy.block_picker_open = true;
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
                    .child(tr(keys::Q_PREFS_COULDNT_LOAD_BLOCKED_USERS)),
            );
        } else if loading && list.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_LOADING)),
            );
        } else if list.is_empty() {
            section = section.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_NO_BLOCKED_USERS)),
            );
        }
        if !list.is_empty() {
            let mut rows = div().flex().flex_col();
            for user_id in &list {
                rows = rows.child(self.blocked_user_row(cx, *user_id));
            }
            if loading && (total as usize) > list.len() {
                rows = rows.child(loading_row("privacy-blocked-loading", cx));
            }
            section = section.child(
                div()
                    .id("privacy-blocked-rows")
                    .max_h(px(BLOCKED_ROWS_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&self.privacy.blocked_scroll)
                    .child(watch(rows, cx, Self::blocked_more)),
            );
        }
        if self.privacy.block_picker_open {
            section = section.child(self.block_picker(cx, &list));
        }
        section.into_any_element()
    }

    /// The next page of blocked users, once the rows are scrolled near
    /// their end (or the loaded ones do not fill the box).
    fn blocked_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(settings) = self.session().map(|s| &s.settings) else {
            return;
        };
        let loaded = settings.blocked_senders.as_ref().map_or(0, Vec::len);
        let has_more =
            settings.blocked_senders.is_some() && (settings.blocked_total.max(0) as usize) > loaded;
        if (settings.blocked_error && !scrolled)
            || !load_due(
                scrolled_near_end(&self.privacy.blocked_scroll),
                has_more,
                settings.blocked_loading,
            )
            || !self.privacy.blocked_asked.allow(loaded, scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_blocked_senders();
        }
        cx.notify();
    }

    /// Slice S3: one blocked user with two-step Unblock (TGX
    /// `SettingsBlockedController` confirms before unblocking).
    fn blocked_user_row(&self, cx: &mut Context<Self>, user_id: i64) -> AnyElement {
        let name = self
            .session()
            .and_then(|s| s.users.get(&user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| tr_with(keys::Q_PREFS_USER, &[("user_id", &user_id.to_string())]));
        let confirming = self.privacy.unblock_confirm == Some(user_id);
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
                            .label(tr(keys::Q_PREFS_UNBLOCK))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.unblock_user(user_id, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("privacy-unblock-cancel-{user_id}"))
                            .small()
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.privacy.unblock_confirm = None;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            row = row.child(
                Button::new(format!("privacy-unblock-{user_id}"))
                    .small()
                    .label(tr(keys::Q_PREFS_UNBLOCK))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.privacy.unblock_confirm = Some(user_id);
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
                .child(tr(keys::Q_PREFS_CHOOSE_A_CONTACT_TO_BLOCK)),
        );
        if rows.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_NO_MORE_CONTACTS)),
            );
        }
        for row in rows {
            let user_id = row.user_id;
            body = body.child(
                div()
                    .id(format!("privacy-block-pick-{user_id}"))
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_PREFS_BLOCK_2,
                        &[("name", &row.name.to_string())],
                    ))
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
        self.privacy.block_picker_open = false;
        if let Some(live) = self.live.as_mut() {
            let result = { live.driver.block_sender(user_id) };
            if let Err(err) = result {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_BLOCK,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            let list = demo.settings.blocked_senders.get_or_insert_with(Vec::new);
            if !list.contains(&user_id) {
                list.push(user_id);
                demo.settings.blocked_total += 1;
            }
        }
        cx.notify();
    }

    /// Slice S3: unblock a user (`setMessageSenderBlockList` with a null
    /// block list — TGX `Tdlib.unblockSender`). Optimistic.
    fn unblock_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        self.privacy.unblock_confirm = None;
        if let Some(live) = self.live.as_mut() {
            let result = { live.driver.unblock_sender(user_id) };
            if let Err(err) = result {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_UNBLOCK,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut()
            && let Some(list) = demo.settings.blocked_senders.as_mut()
        {
            list.retain(|id| *id != user_id);
            demo.settings.blocked_total = demo.settings.blocked_total.saturating_sub(1);
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
        let target = self.privacy.editor?;
        match target {
            PrivacyEditorTarget::NewChat => return Some(self.new_chat_editor(cx)),
            PrivacyEditorTarget::FileOpen => return Some(self.file_open_editor(cx)),
            _ => {}
        }
        let session = self.session();
        let current: Option<PrivacyWho> = match target {
            PrivacyEditorTarget::Rule(key) => session
                .and_then(|s| s.settings.privacy.get(&key))
                .and_then(|st| match st {
                    PrivacyKeyState::Ready(d) => d.who,
                    _ => None,
                }),
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
                    .and_then(|s| s.settings.privacy.get(&key))
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
                && !session.is_some_and(|s| s.payments.premium_option == Some(true))
            {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_PREFS_RESTRICTING_WHO_CAN_SEND_YOU)),
                );
            }
        }
        Some(self.privacy_shell(cx, "editor", target.label(), body.into_any_element()))
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
        session.settings.privacy.insert(
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
        PrivacySettingKey::AllowCalls,
        PrivacyWho::Contacts,
        &[61],
        &[63],
    );
    ready(
        PrivacySettingKey::PeerToPeer,
        PrivacyWho::Everybody,
        &[],
        &[],
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
        .settings
        .privacy
        .get_mut(&PrivacySettingKey::AllowChatInvites)
    {
        detail.allow_premium = true;
        detail.always_chats = vec![9001];
    }
    if let Some(PrivacyKeyState::Ready(detail)) = session
        .settings
        .privacy
        .get_mut(&PrivacySettingKey::AutosaveGifts)
    {
        detail.never_bots = true;
    }
    session.my_user_id = Some(60);
    session.payments.premium_option = Some(true);
    session.users_state.user_full_infos.insert(
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
    session.settings.privacy_data.new_chat = Some(quill::privacy::NewChatPrivacyState::Ready(
        quill::privacy::NewChatPrivacy {
            allow_from_unknown: false,
            incoming_paid_message_star_count: 0,
        },
    ));
    session.settings.privacy_data.can_ignore_sensitive = true;
    session.settings.privacy_data.ignore_sensitive = Some(false);
    session.settings.privacy_data.check_password_suggested = true;
    session.settings.privacy_data.inactive_session_ttl_days = Some(180);
    session.settings.read_date_show = Some(true);
    session.chat_list.archive_chat_list_settings =
        Some(quill::telegram::requests::ArchiveChatListSettings {
            archive_and_mute_new_chats_from_unknown_users: true,
            keep_unmuted_chats_archived: false,
            keep_chats_from_folders_archived: true,
        });
    session.calls.privacy_allow_calls = Some(PrivacyWho::Contacts);
    session.calls.privacy_p2p = Some(PrivacyWho::Everybody);
    session.settings.blocked_senders = Some(vec![63]);
    session.settings.blocked_total = 1;
    session.users_state.contacts = Some(vec![61, 62, 63]);
}
/// Slice S3: the Privacy-screen row value for one rule key: the base
/// choice, with the always/never exception counts when non-zero (TGX
/// `SettingsPrivacyKeyController` shows the mode plus exceptions).
pub(crate) fn privacy_key_value(session: &Session, key: PrivacySettingKey) -> String {
    match session.settings.privacy.get(&key) {
        None | Some(PrivacyKeyState::Loading) => tr(keys::Q_PREFS_LOADING).to_string(),
        Some(PrivacyKeyState::Failed) => tr(keys::Q_PREFS_COULDNT_LOAD).to_string(),
        Some(PrivacyKeyState::Ready(detail)) => {
            let mut value = detail
                .who
                .map(|w| w.label().to_string())
                .unwrap_or_else(|| tr(keys::Q_PREFS_CUSTOM_2).to_string());
            let (always, never) = detail.exception_counts();
            if always > 0 || never > 0 {
                value.push_str(&format!(" (+{always}/−{never})"));
            }
            value
        }
    }
}

/// B13: the "Who can message me" row value (tdesktop Messages privacy).
pub(crate) fn new_chat_privacy_value(session: &Session) -> String {
    match session.settings.privacy_data.new_chat {
        None | Some(quill::privacy::NewChatPrivacyState::Loading) => {
            tr(keys::Q_PREFS_LOADING).to_string()
        }
        Some(quill::privacy::NewChatPrivacyState::Failed) => {
            tr(keys::Q_PREFS_COULDNT_LOAD).to_string()
        }
        Some(quill::privacy::NewChatPrivacyState::Ready(settings)) => {
            if settings.incoming_paid_message_star_count > 0 {
                tr_with(
                    keys::Q_PREFS_STARS_PER_MESSAGE,
                    &[(
                        "incoming_paid_message_star_count",
                        &settings.incoming_paid_message_star_count.to_string(),
                    )],
                )
            } else if settings.allow_from_unknown {
                tr(keys::EDIT_PRIVACY_EVERYONE).to_string()
            } else {
                tr(keys::EDIT_PRIVACY_CONTACTS_AND_PREMIUM).to_string()
            }
        }
    }
}

mod privacy_radio_row;

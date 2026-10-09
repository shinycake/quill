//! B13: the Privacy and Security additions next to the rule screen
//! (tdesktop `Settings::PrivacySecurity`): "Who can message me", the gift
//! settings, "find me by my number", the 18+ switch, "File open
//! confirmations" with the open-time warning dialog, and the
//! "Do you still remember your password?" check.

use super::app::QuillApp;
use super::chat_theme::{danger, text_muted};
use super::privacy::{PrivacyEditorTarget, new_chat_privacy_value};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::file_prefs::{self, OpenWarning};
use quill::privacy::{GiftSettings, NewChatPrivacyState};
use quill::state::PasswordCheck;
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use zeroize::Zeroize;

/// A file waiting for the open-time warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileOpenConfirm {
    pub path: PathBuf,
    pub warning: OpenWarning,
}

/// Transient state of the B13 privacy / security UI.
pub(crate) struct PrivacyUi {
    /// The extension whitelist editor.
    pub file_ext_input: Entity<TextareaState>,
    /// Draft of the IP-reveal switch inside the whitelist editor.
    pub file_open_ip_draft: bool,
    /// The exception picker lists groups instead of contacts.
    pub exception_picker_groups: bool,
    /// The open-time warning, when one is showing.
    pub file_open: Option<FileOpenConfirm>,
    /// "Remember for this file type" / "Don't ask me again" ticked.
    pub file_open_remember: bool,
    /// The session details view inside the sessions dialog.
    pub session_details: Option<i64>,
    /// "Reset statistics" is awaiting its confirmation.
    pub network_reset_confirm: bool,
}

impl PrivacyUi {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let file_ext_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("pdf txt log")
                .auto_grow(2, 6)
                .submit_on_enter(false)
        });
        Self {
            file_ext_input,
            file_open_ip_draft: true,
            exception_picker_groups: false,
            file_open: None,
            file_open_remember: false,
            session_details: None,
            network_reset_confirm: false,
        }
    }
}

impl QuillApp {
    /// A radio-style row: a dot, a label, an optional second line.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn choice_row(
        &self,
        cx: &mut Context<Self>,
        id: String,
        label: &'static str,
        about: Option<String>,
        selected: bool,
        on_pick: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) -> AnyElement {
        let mut text = div().flex().flex_col().child(div().text_sm().child(label));
        if let Some(about) = about {
            text = text.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(about),
            );
        }
        div()
            .id(id)
            .role(gpui_kit::Role::Button)
            .aria_label(label)
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
            .child(text)
            .on_click(cx.listener(move |this, _, _, cx| on_pick(this, cx)))
            .into_any_element()
    }

    /// The "Who can message me" row of the main overlay.
    pub(super) fn new_chat_privacy_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self
            .session()
            .map(new_chat_privacy_value)
            .unwrap_or_default();
        div()
            .id("privacy-rule-new-chat")
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Messages · {value}"))
            .tab_index(0)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_md()
            .child(div().text_sm().child("Messages"))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(value),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.privacy_editor = Some(PrivacyEditorTarget::NewChat);
                this.exception_picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Messages privacy editor (tdesktop `EditMessagesPrivacyBox`):
    /// everybody, or contacts and Premium users only.
    pub(super) fn new_chat_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let state = session.and_then(|s| s.privacy_data.new_chat);
        let premium = session.is_some_and(|s| s.premium_option == Some(true));
        let error = session.and_then(|s| s.privacy_data.error.clone());
        let mut body = div().flex().flex_col().gap_1();
        body = body.child(
            div()
                .text_xs()
                .font_semibold()
                .px_1()
                .text_color(cx.theme().muted_foreground)
                .child("Who can send me messages"),
        );
        match state {
            Some(NewChatPrivacyState::Ready(settings)) => {
                let everyone = settings.allow_from_unknown;
                body = body
                    .child(self.choice_row(
                        cx,
                        "privacy-new-chat-everyone".into(),
                        "Everybody",
                        None,
                        everyone,
                        |this, cx| this.set_new_chat_allow(true, cx),
                    ))
                    .child(
                        self.choice_row(
                            cx,
                            "privacy-new-chat-restricted".into(),
                            "My contacts and Premium users",
                            Some(
                                "Non-contacts can message you only if they have Telegram Premium."
                                    .into(),
                            ),
                            !everyone,
                            |this, cx| this.set_new_chat_allow(false, cx),
                        ),
                    );
                if settings.incoming_paid_message_star_count > 0 {
                    body = body.child(
                        div()
                            .text_xs()
                            .px_2()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "Non-contacts pay {} Stars per message to write to you. Change the price in the Telegram mobile app.",
                                settings.incoming_paid_message_star_count
                            )),
                    );
                }
                if !premium {
                    body = body.child(
                        div()
                            .text_xs()
                            .px_2()
                            .text_color(cx.theme().muted_foreground)
                            .child("Restricting new chats needs Telegram Premium."),
                    );
                }
            }
            Some(NewChatPrivacyState::Failed) => {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("Couldn't load this setting."),
                );
            }
            _ => {
                body = body.child(
                    div()
                        .text_xs()
                        .px_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading…"),
                );
            }
        }
        if let Some(line) = error {
            body = body.child(div().text_xs().px_2().text_color(danger()).child(line));
        }
        self.privacy_shell(cx, "editor", "Who can message me", body.into_any_element())
    }

    /// Choose Everybody (`true`) or Contacts and Premium (`false`).
    pub(super) fn set_new_chat_allow(&mut self, allow: bool, cx: &mut Context<Self>) {
        let premium = self
            .session()
            .is_some_and(|s| s.premium_option == Some(true));
        if !allow && !premium {
            self.status_note = "Restricting new chats needs Telegram Premium.".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_new_chat_privacy(allow) {
                self.status_note = format!("privacy update failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut()
            && let Some(NewChatPrivacyState::Ready(current)) = demo.privacy_data.new_chat
        {
            demo.set_new_chat_privacy_local(quill::privacy::NewChatPrivacy {
                allow_from_unknown: allow,
                ..current
            });
        }
        cx.notify();
    }

    /// The "find me by my number" choice under a hidden phone number.
    pub(super) fn find_by_number_block(&self, cx: &mut Context<Self>) -> AnyElement {
        let key = PrivacySettingKey::AllowFindingByPhoneNumber;
        let target = PrivacyEditorTarget::Rule(key);
        let current: Option<PrivacyWho> = self
            .session()
            .and_then(|s| s.privacy.get(&key))
            .and_then(|st| match st {
                quill::privacy::PrivacyKeyState::Ready(d) => d.who,
                _ => None,
            });
        let mut block = div().flex().flex_col().gap_1().mt_2();
        block = block.child(
            div()
                .text_xs()
                .font_semibold()
                .px_1()
                .text_color(cx.theme().muted_foreground)
                .child(key.label()),
        );
        for &who in key.options() {
            block = block.child(self.privacy_radio_row(cx, target, who, current));
        }
        block.into_any_element()
    }

    /// Gift settings shown inside the Gifts editor (tdesktop
    /// `GiftsAutoSavePrivacyController`): the gift button in chats and the
    /// accepted gift kinds. Changing them needs Premium.
    pub(super) fn gift_settings_block(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let settings = session
            .and_then(|s| s.my_gift_settings())
            .unwrap_or_default();
        let premium = session.is_some_and(|s| s.premium_option == Some(true));
        let mut block = div().flex().flex_col().gap_1().mt_1();
        block = block.child(self.gift_switch(
            cx,
            "gift-show-button",
            "Show Gift Icon in Chats",
            Some("Display the gift icon in the message field for both participants in all chats."),
            settings.show_gift_button,
            |g, on| g.show_gift_button = on,
        ));
        block = block.child(
            div()
                .text_xs()
                .font_semibold()
                .px_1()
                .mt_1()
                .text_color(cx.theme().muted_foreground)
                .child("Accepted Gift Types"),
        );
        let types: [(
            &'static str,
            &'static str,
            bool,
            fn(&mut GiftSettings, bool),
        ); 5] = [
            (
                "gift-type-unlimited",
                "Unlimited",
                settings.unlimited_gifts,
                |g, on| g.unlimited_gifts = on,
            ),
            (
                "gift-type-limited",
                "Limited-Edition",
                settings.limited_gifts,
                |g, on| g.limited_gifts = on,
            ),
            (
                "gift-type-unique",
                "Unique",
                settings.upgraded_gifts,
                |g, on| g.upgraded_gifts = on,
            ),
            (
                "gift-type-channels",
                "From Channels",
                settings.gifts_from_channels,
                |g, on| g.gifts_from_channels = on,
            ),
            (
                "gift-type-premium",
                "Premium Subscriptions",
                settings.premium_subscription,
                |g, on| g.premium_subscription = on,
            ),
        ];
        for (id, label, on, apply) in types {
            block = block.child(self.gift_switch(cx, id, label, None, on, apply));
        }
        block = block.child(
            div()
                .text_xs()
                .px_2()
                .text_color(cx.theme().muted_foreground)
                .child(if premium {
                    "Choose the types of gifts that you accept."
                } else {
                    "Choose the types of gifts that you accept. Changing them needs Telegram Premium."
                }),
        );
        block.into_any_element()
    }

    fn gift_switch(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        about: Option<&'static str>,
        on: bool,
        apply: fn(&mut GiftSettings, bool),
    ) -> AnyElement {
        let mut text = div().flex().flex_col().child(div().text_sm().child(label));
        if let Some(about) = about {
            text = text.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(about),
            );
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .child(text)
            .child(
                Switch::new(id)
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                        this.update_gift_settings(|g| apply(g, next), cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn update_gift_settings(
        &mut self,
        edit: impl FnOnce(&mut GiftSettings),
        cx: &mut Context<Self>,
    ) {
        let premium = self
            .session()
            .is_some_and(|s| s.premium_option == Some(true));
        if !premium {
            self.status_note = "Changing gift settings needs Telegram Premium.".into();
            cx.notify();
            return;
        }
        let mut next = self
            .session()
            .and_then(|s| s.my_gift_settings())
            .unwrap_or_default();
        edit(&mut next);
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_gift_settings(next) {
                self.status_note = format!("gift settings update failed: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.set_my_gift_settings_local(next);
        }
        cx.notify();
    }

    /// "Security" block of the main overlay: the 18+ switch (when the
    /// account may turn it on) and the file-open confirmations row.
    pub(super) fn privacy_security_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let can_ignore = session.is_some_and(|s| s.privacy_data.can_ignore_sensitive);
        let ignore = session
            .and_then(|s| s.privacy_data.ignore_sensitive)
            .unwrap_or(false);
        let error = session.and_then(|s| s.privacy_data.error.clone());
        let mut section = div().flex().flex_col().gap_1();
        section = section.child(
            div()
                .text_sm()
                .font_semibold()
                .px_1()
                .child("Security and content"),
        );
        if can_ignore {
            section = section.child(
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
                            .child(div().text_sm().child("Show 18+ Content"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Do not hide media that contains content suitable only for adults.",
                                    ),
                            ),
                    )
                    .child(
                        Switch::new("privacy-sensitive-content")
                            .checked(ignore)
                            .accessibility_label("Show 18+ Content")
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                this.set_sensitive_content(on, cx);
                            })),
                    ),
            );
        }
        let prefs = file_prefs::current();
        let summary = if prefs.no_warning_extensions.is_empty() && prefs.ip_reveal_warning {
            "Default".to_string()
        } else {
            format!("{} trusted", prefs.no_warning_extensions.len())
        };
        section = section.child(
            div()
                .id("privacy-file-open")
                .role(gpui_kit::Role::Button)
                .aria_label("File open confirmations")
                .tab_index(0)
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .rounded_md()
                .child(div().text_sm().child("File open confirmations"))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(summary),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_file_open_editor(window, cx);
                })),
        );
        if let Some(line) = error {
            section = section.child(div().text_xs().px_2().text_color(danger()).child(line));
        }
        section.into_any_element()
    }

    /// `setOption(ignore_sensitive_content_restrictions)`. Live: TDLib
    /// answers with `updateOption`. Demo: flips the fixture.
    pub(super) fn set_sensitive_content(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_ignore_sensitive_content(on) {
                self.status_note = format!("couldn't change the 18+ setting: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.privacy_data.ignore_sensitive = Some(on);
        }
        cx.notify();
    }

    /// Open the whitelist editor, loading the saved values.
    fn open_file_open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = file_prefs::current();
        let text = file_prefs::format_extension_list(&prefs.no_warning_extensions);
        self.privacy_ui
            .file_ext_input
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        self.privacy_ui.file_open_ip_draft = prefs.ip_reveal_warning;
        self.privacy_editor = Some(PrivacyEditorTarget::FileOpen);
        cx.notify();
    }

    /// "File open confirmations" (tdesktop `OpenFileConfirmationsBox`):
    /// the extension whitelist and the IP-reveal switch, saved together.
    pub(super) fn file_open_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let ip = self.privacy_ui.file_open_ip_draft;
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .px_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("Extensions whitelist"),
            )
            .child(
                Textarea::new(&self.privacy_ui.file_ext_input)
                    .aria_label("Extensions whitelist")
                    .h(px(72.)),
            )
            .child(
                div()
                    .text_xs()
                    .px_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("Open files with the following extensions without additional confirmation."),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child("IP reveal warning"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Show confirmation when opening files that may reveal your IP address.",
                                    ),
                            ),
                    )
                    .child(
                        Switch::new("privacy-ip-reveal")
                            .checked(ip)
                            .accessibility_label("IP reveal warning")
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                this.privacy_ui.file_open_ip_draft = on;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("privacy-file-open-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.privacy_editor = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("privacy-file-open-save")
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_file_open_settings(cx);
                            })),
                    ),
            )
            .into_any_element();
        self.privacy_shell(cx, "editor", "File open confirmations", body)
    }

    fn save_file_open_settings(&mut self, cx: &mut Context<Self>) {
        let text = self.privacy_ui.file_ext_input.read(cx).value().to_string();
        let ip = self.privacy_ui.file_open_ip_draft;
        let extensions = file_prefs::parse_extension_list(&text);
        file_prefs::update(|prefs| {
            prefs.no_warning_extensions = extensions;
            prefs.ip_reveal_warning = ip;
        });
        self.privacy_editor = None;
        self.status_note = "file open settings saved".into();
        cx.notify();
    }

    /// Open a downloaded file with the system app, asking first when
    /// tdesktop would (`LaunchWithWarning`). The sender-verified
    /// exemption is not applied: callers have no message at hand.
    pub(super) fn open_file_guarded(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let prefs = file_prefs::current();
        match file_prefs::open_warning(&path, &prefs, false) {
            None => self.open_file_now(&path, cx),
            Some(warning) => {
                self.privacy_ui.file_open_remember = false;
                self.privacy_ui.file_open = Some(FileOpenConfirm { path, warning });
                cx.notify();
            }
        }
    }

    fn open_file_now(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.status_note = if quill::platform::open_local_file(path) {
            "opened file".into()
        } else {
            "could not open the file".into()
        };
        cx.notify();
    }

    /// The warning's confirm: remember the choice when ticked, then open.
    pub(super) fn confirm_file_open(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.privacy_ui.file_open.take() else {
            return;
        };
        if self.privacy_ui.file_open_remember {
            let extension = file_prefs::file_extension(&confirm.path);
            file_prefs::update(|prefs| match confirm.warning {
                OpenWarning::IpReveal => prefs.ip_reveal_warning = false,
                OpenWarning::Executable | OpenWarning::Unknown => {
                    if !extension.is_empty() {
                        prefs.no_warning_extensions.insert(extension);
                    }
                }
            });
        }
        self.open_file_now(&confirm.path, cx);
    }

    /// kit `Dialog` for the file-open warning (`DialogKind::FileOpenConfirm`).
    pub(super) fn build_file_open_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FileOpenConfirm, |this, _, cx| {
                this.privacy_ui.file_open = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let confirm = this.privacy_ui.file_open.clone();
            let (text, confirm_label, remember_label) = match confirm.as_ref() {
                Some(c) => {
                    let extension = file_prefs::file_extension(&c.path);
                    let dotted = if extension.is_empty() {
                        "no extension".to_string()
                    } else {
                        format!(".{extension}")
                    };
                    match c.warning {
                        OpenWarning::Executable => (
                            format!(
                                "This file has the extension {dotted}\nIt may harm your computer.\nAre you sure you want to run it?"
                            ),
                            "Run",
                            "Remember for this file type",
                        ),
                        OpenWarning::Unknown => (
                            format!(
                                "This file has {dotted} extension.\nAre you sure you want to open it?"
                            ),
                            "Open",
                            "Remember for this file type",
                        ),
                        OpenWarning::IpReveal => (
                            "Opening this file can potentially expose your IP address to its creator. Continue?"
                                .to_string(),
                            "Open",
                            "Don't ask me again",
                        ),
                    }
                }
                None => (String::new(), "Open", "Remember for this file type"),
            };
            let name = confirm
                .as_ref()
                .and_then(|c| c.path.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let remember = this.privacy_ui.file_open_remember;
            let can_remember = confirm.as_ref().is_some_and(|c| {
                c.warning == OpenWarning::IpReveal
                    || !file_prefs::file_extension(&c.path).is_empty()
            });
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().font_semibold().child(name))
                .child(div().text_sm().child(text))
                .when(can_remember, |this_| {
                    this_
                        .child(
                            Checkbox::new("file-open-remember")
                                .label(remember_label)
                                .checked(remember)
                                .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                    this.privacy_ui.file_open_remember = on;
                                    cx.notify();
                                })),
                        )
                        .when(remember, |this_| {
                            this_.child(div().text_xs().text_color(text_muted()).child(
                                "You can later edit trusted file types in Settings > Privacy > File open confirmations.",
                            ))
                        })
                })
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("file-open-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.privacy_ui.file_open = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FileOpenConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("file-open-confirm")
                        .label(confirm_label)
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_file_open(cx);
                            this.close_kit_dialog_if_done(DialogKind::FileOpenConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Open this file?"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Save a copy of `src` where the user picks (the "Ask where to save
    /// each file" path). Works on every platform through the GPUI
    /// save-file prompt.
    pub(super) fn save_file_asking(&mut self, src: PathBuf, cx: &mut Context<Self>) {
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let dir = quill::media_viewer::downloads_dir().unwrap_or_else(|| PathBuf::from("."));
        let picker = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = picker.await else {
                return;
            };
            let copied = cx
                .background_executor()
                .spawn(async move { std::fs::copy(&src, &dest).map(|_| dest) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status_note = match copied {
                    Ok(dest) => format!(
                        "saved to {}",
                        dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                    ),
                    Err(err) => format!("couldn't save: {err}"),
                };
                cx.notify();
            });
        })
        .detach();
    }

    // ---- "Do you still remember your password?" ----

    /// The password check card (tdesktop's "Your password" suggestion):
    /// a password field, "Check" and "Dismiss". A right password shows
    /// the finish step; "Done" tells the server to stop suggesting it.
    pub(super) fn password_check_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let check = self
            .session()
            .map(|s| s.privacy_data.password_check)
            .unwrap_or_default();
        let mut card = div()
            .id("password-check-card")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border);
        if check == PasswordCheck::Remembered {
            return card
                .child(div().text_sm().font_semibold().child("Perfect!"))
                .child(div().text_xs().child("You still remember your password."))
                .child(
                    div().flex().justify_end().child(
                        Button::new("password-check-done")
                            .label("Done")
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_password_check(cx);
                            })),
                    ),
                )
                .into_any_element();
        }
        card = card
            .child(div().text_sm().font_semibold().child("Your password"))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(
                "Your account is protected by 2-Step Verification. Do you still remember your password?",
            ))
            .child(
                Input::new(&self.twofa_current_password)
                    .aria_label("Your two-step verification password")
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            );
        match check {
            PasswordCheck::Wrong => {
                card = card.child(div().text_xs().text_color(danger()).child(
                    "That password is wrong. Try again, or change it in Two-Step Verification.",
                ));
            }
            PasswordCheck::Failed => {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child("Couldn't check the password. Try again later."),
                );
            }
            _ => {}
        }
        card.child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("password-check-dismiss")
                        .label("Dismiss")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismiss_password_check(cx);
                        })),
                )
                .child(
                    Button::new("password-check-submit")
                        .label("Check")
                        .primary()
                        .disabled(check == PasswordCheck::Checking)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_password_check(window, cx);
                        })),
                ),
        )
        .into_any_element()
    }

    /// Send the typed password to be verified; the field is cleared and
    /// the string zeroized right away.
    fn submit_password_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut password = self.twofa_current_password.read(cx).value().to_string();
        self.twofa_current_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        if password.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_remembered_password(&password);
        } else if let Some(demo) = self.demo_session.as_mut() {
            // No server in the demo: any password "matches".
            demo.privacy_data.password_check = PasswordCheck::Remembered;
        }
        password.zeroize();
        cx.notify();
    }

    /// Dismiss the suggestion (`hideSuggestedAction`) and reset the card.
    fn dismiss_password_check(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.hide_check_password_suggestion();
            live.driver.session.privacy_data.password_check = PasswordCheck::Idle;
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.privacy_data.check_password_suggested = false;
            demo.privacy_data.password_check = PasswordCheck::Idle;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::new_chat_privacy_value;
    use quill::diagnostics::{DiagnosticSink, MemorySink};
    use quill::ids::AccountKey;
    use quill::privacy::{NewChatPrivacy, NewChatPrivacyState};
    use quill::state::Session;
    use std::sync::Arc;

    fn session() -> Session {
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        Session::new(AccountKey::primary(), sink)
    }

    #[test]
    fn messages_row_value_follows_the_server_state() {
        let mut session = session();
        assert_eq!(new_chat_privacy_value(&session), "Loading…");
        session.privacy_data.new_chat = Some(NewChatPrivacyState::Failed);
        assert_eq!(new_chat_privacy_value(&session), "Couldn't load");
        let ready = |allow, stars| {
            Some(NewChatPrivacyState::Ready(NewChatPrivacy {
                allow_from_unknown: allow,
                incoming_paid_message_star_count: stars,
            }))
        };
        session.privacy_data.new_chat = ready(true, 0);
        assert_eq!(new_chat_privacy_value(&session), "Everybody");
        session.privacy_data.new_chat = ready(false, 0);
        assert_eq!(new_chat_privacy_value(&session), "Contacts & Premium");
        session.privacy_data.new_chat = ready(false, 15);
        assert_eq!(new_chat_privacy_value(&session), "15 Stars per message");
    }
}

/// Effects of the privacy actions on a demo `QuillApp` (needs the
/// `demo-capture` feature for gpui-kit's test support).
#[cfg(all(test, feature = "demo-capture"))]
mod dispatch_tests {
    use super::FileOpenConfirm;
    use crate::ui::app::QuillApp;
    use crate::ui::keybindings::bind_keys;
    use crate::ui::privacy::ExceptionFlag;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use crate::ui::{PrivacyEditorTarget, PrivacyExceptionKind};
    use gpui_kit::component::Root;
    use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
    use quill::file_prefs::{self, FilePrefs, OpenWarning};
    use quill::privacy::{NewChatPrivacyState, PrivacyKeyState};
    use quill::telegram::requests::PrivacyWho;
    use quill::telegram::requests_privacy::PrivacySettingKey;

    fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::ReadyPrivacy))
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        (app, vcx)
    }

    fn rule_who(app: &QuillApp, key: PrivacySettingKey) -> Option<PrivacyWho> {
        match app.demo_session.as_ref()?.privacy.get(&key)? {
            PrivacyKeyState::Ready(detail) => detail.who,
            _ => None,
        }
    }

    #[gpui_kit::test]
    fn gift_settings_need_premium_and_apply_in_the_demo(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let limited = |app: &QuillApp| {
            app.demo_session
                .as_ref()
                .and_then(|s| s.my_gift_settings())
                .map(|g| g.limited_gifts)
        };
        assert_eq!(app.read_with(vcx, |app, _| limited(app)), Some(false));
        app.update_in(vcx, |app, _, cx| {
            app.update_gift_settings(|g| g.limited_gifts = true, cx);
        });
        assert_eq!(app.read_with(vcx, |app, _| limited(app)), Some(true));
        // Without Premium the change is refused and says why.
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.premium_option = Some(false);
            }
            app.update_gift_settings(|g| g.limited_gifts = false, cx);
        });
        assert_eq!(app.read_with(vcx, |app, _| limited(app)), Some(true));
        let note = app.read_with(vcx, |app, _| app.status_note.clone());
        assert!(note.contains("Premium"), "{note}");
    }

    #[gpui_kit::test]
    fn messages_privacy_and_sensitive_switch_update_the_demo(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        app.update_in(vcx, |app, _, cx| {
            app.set_new_chat_allow(true, cx);
            app.set_sensitive_content(true, cx);
        });
        app.read_with(vcx, |app, _| {
            let data = &app.demo_session.as_ref().unwrap().privacy_data;
            assert!(matches!(
                data.new_chat,
                Some(NewChatPrivacyState::Ready(s)) if s.allow_from_unknown
            ));
            assert_eq!(data.ignore_sensitive, Some(true));
        });
        // Restricting needs Premium.
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.premium_option = Some(false);
            }
            app.set_new_chat_allow(false, cx);
        });
        app.read_with(vcx, |app, _| {
            let data = &app.demo_session.as_ref().unwrap().privacy_data;
            assert!(matches!(
                data.new_chat,
                Some(NewChatPrivacyState::Ready(s)) if s.allow_from_unknown
            ));
        });
    }

    #[gpui_kit::test]
    fn voice_message_restriction_is_refused_without_premium(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let key = PrivacySettingKey::AllowVoiceMessages;
        let target = PrivacyEditorTarget::Rule(key);
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.premium_option = Some(false);
            }
            app.set_privacy_target_who(target, PrivacyWho::Nobody, cx);
        });
        assert_eq!(
            app.read_with(vcx, |app, _| rule_who(app, key)),
            Some(PrivacyWho::Everybody)
        );
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.premium_option = Some(true);
            }
            app.set_privacy_target_who(target, PrivacyWho::Nobody, cx);
        });
        assert_eq!(
            app.read_with(vcx, |app, _| rule_who(app, key)),
            Some(PrivacyWho::Nobody)
        );
    }

    #[gpui_kit::test]
    fn exception_rows_edit_premium_bots_and_groups(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let invites = PrivacyEditorTarget::Rule(PrivacySettingKey::AllowChatInvites);
        let gifts = PrivacyEditorTarget::Rule(PrivacySettingKey::AutosaveGifts);
        app.update_in(vcx, |app, _, cx| {
            app.edit_exception_flag(
                invites,
                PrivacyExceptionKind::Always,
                ExceptionFlag::Premium,
                false,
                cx,
            );
            app.edit_exception_flag(
                gifts,
                PrivacyExceptionKind::Always,
                ExceptionFlag::Bots,
                true,
                cx,
            );
            app.edit_exception_chat(invites, PrivacyExceptionKind::Never, 9001, true, cx);
        });
        app.read_with(vcx, |app, _| {
            let session = app.demo_session.as_ref().unwrap();
            let detail = |key| match session.privacy.get(&key) {
                Some(PrivacyKeyState::Ready(d)) => d.clone(),
                other => panic!("{other:?}"),
            };
            let invites = detail(PrivacySettingKey::AllowChatInvites);
            assert!(!invites.allow_premium);
            // Moving a group to Never takes it out of Always.
            assert_eq!(invites.never_chats, vec![9001]);
            assert!(invites.always_chats.is_empty());
            let gifts = detail(PrivacySettingKey::AutosaveGifts);
            // Choosing Mini Apps for Always clears it from Never.
            assert!(gifts.allow_bots && !gifts.never_bots);
        });
    }

    #[gpui_kit::test]
    fn file_open_warning_remembers_the_choice(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        // In-memory only: never read or write the real file_prefs.json.
        file_prefs::set_persistence(false);
        file_prefs::store(FilePrefs::default());
        let path = std::path::PathBuf::from("/nonexistent/setup.bin");
        app.update_in(vcx, |app, _, cx| {
            app.open_file_guarded(path.clone(), cx);
        });
        app.read_with(vcx, |app, _| {
            assert_eq!(
                app.privacy_ui.file_open,
                Some(FileOpenConfirm {
                    path: path.clone(),
                    warning: OpenWarning::Executable
                })
            );
        });
        // Confirm without ticking "remember": nothing is trusted.
        app.update_in(vcx, |app, _, cx| app.confirm_file_open(cx));
        assert!(file_prefs::current().no_warning_extensions.is_empty());
        // Confirm with it ticked: the extension opens directly afterwards.
        app.update_in(vcx, |app, _, cx| {
            app.open_file_guarded(path.clone(), cx);
            app.privacy_ui.file_open_remember = true;
            app.confirm_file_open(cx);
        });
        assert!(file_prefs::current().no_warning_extensions.contains("bin"));
        app.update_in(vcx, |app, _, cx| {
            app.open_file_guarded(path.clone(), cx);
        });
        app.read_with(vcx, |app, _| {
            assert!(
                app.privacy_ui.file_open.is_none(),
                "trusted type opens directly"
            );
        });
        file_prefs::store(FilePrefs::default());
    }
}

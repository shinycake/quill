//! Passkeys in Privacy and Security (tdesktop `settings_passkeys.cpp` and
//! the Passkeys row of `settings_privacy_security.cpp`): the row, the list
//! with each passkey's menu, the delete confirmation and the "Protect your
//! account" box. Rules and requests: `quill::passkeys`.
//!
//! The three surfaces are separate kit dialogs. The confirmation and the
//! box win over the list by priority, so closing one shows the list again,
//! the way a tdesktop box closes back to its section.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::chat_theme::danger;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::TestSupportExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::{Dialog, DialogContent};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::local_time::{civil_local, now_unix};
use quill::passkeys::{Passkey, PasskeysState, RowTarget, delete_question};
use quill::power_saving;

const TITLE: quill::lang::Key = keys::SETTINGS_PASSKEYS_TITLE;
const ABOUT: quill::lang::Key = keys::SETTINGS_PASSKEYS_ABOUT;
const BUTTON_ABOUT: quill::lang::Key = keys::Q_PREFS_YOUR_PASSKEY_IS_STORED_SECURELY;
const ICON_SIZE: f32 = 32.;

/// Which passkey surfaces are open.
#[derive(Debug, Default)]
pub(in crate::ui) struct PasskeysUi {
    /// The Passkeys list.
    pub(in crate::ui) list_open: bool,
    /// The "Protect your account" box (`PasskeysNoneBox`).
    pub(in crate::ui) about_open: bool,
    /// The passkey the delete confirmation asks about.
    pub(in crate::ui) confirm: Option<String>,
    /// The ceremony dialog over the box (`passkey_ceremony`).
    pub(in crate::ui) ceremony: Option<super::passkey_ceremony::CeremonyUi>,
    /// The PIN field of the ceremony, kept so it is made once.
    pub(in crate::ui) pin_input: Option<Entity<gpui_kit::component::input::InputState>>,
    /// Whether the ceremony ticker runs.
    pub(in crate::ui) ticking: bool,
    /// Demos without a session: show "Log in using passkey".
    pub(in crate::ui) demo_login_link: bool,
    /// Demos without a session: the error line under the link.
    pub(in crate::ui) demo_login_error: Option<String>,
}

impl QuillApp {
    fn passkeys_state(&self) -> Option<&PasskeysState> {
        self.session().map(|s| &s.settings.passkeys)
    }

    /// A look at the passkeys: asks for the list unless it is fresh.
    pub(in crate::ui) fn request_passkeys(&mut self) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.request_passkeys();
        }
    }

    /// The Passkeys row of Privacy and Security, when it shows.
    pub(in crate::ui) fn passkeys_settings_row(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.passkeys_state()?;
        if !state.row_shown() {
            return None;
        }
        let label = state.row_label().text();
        Some(
            Button::new("setting-Passkeys")
                .label(tr(TITLE))
                .ghost()
                .child(
                    div()
                        .ml_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    // Nothing opens while the list is loading (tdesktop).
                    let Some(target) = this.passkeys_state().and_then(PasskeysState::row_target)
                    else {
                        return;
                    };
                    match target {
                        RowTarget::About => this.open_passkeys_about(cx),
                        RowTarget::List => this.open_passkeys(cx),
                    }
                }))
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn open_passkeys(&mut self, cx: &mut Context<Self>) {
        let ui = &mut self.privacy.passkeys;
        ui.list_open = true;
        ui.confirm = None;
        self.request_passkeys();
        cx.notify();
    }

    pub(in crate::ui) fn open_passkeys_about(&mut self, cx: &mut Context<Self>) {
        self.privacy.passkeys.about_open = true;
        cx.notify();
    }

    fn close_passkeys(&mut self, cx: &mut Context<Self>) {
        let ui = &mut self.privacy.passkeys;
        *ui = PasskeysUi {
            pin_input: ui.pin_input.take(),
            ticking: ui.ticking,
            ..PasskeysUi::default()
        };
        cx.notify();
    }

    pub(in crate::ui) fn ask_delete_passkey(&mut self, passkey_id: String, cx: &mut Context<Self>) {
        if let Some(session) = self.live_or_demo_session_mut() {
            session.settings.passkeys.error = None;
        }
        self.privacy.passkeys.confirm = Some(passkey_id);
        cx.notify();
    }

    /// Cancel / Esc on the confirmation: back to the list, without the
    /// failed attempt's error.
    fn dismiss_delete_passkey(&mut self, cx: &mut Context<Self>) {
        self.privacy.passkeys.confirm = None;
        if let Some(session) = self.live_or_demo_session_mut()
            && session.settings.passkeys.removing.is_none()
        {
            session.settings.passkeys.error = None;
        }
        cx.notify();
    }

    fn live_or_demo_session_mut(&mut self) -> Option<&mut quill::state::Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// The confirmation's Delete: `removeLoginPasskey`. The confirmation
    /// closes once the server confirmed (the passkey left the list).
    pub(in crate::ui) fn confirm_delete_passkey(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.privacy.passkeys.confirm.clone() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.remove_passkey(&id);
        } else if let Some(session) = self.demo_session.as_mut() {
            // The demo has no server: the removal and the refetch at once.
            let passkeys = &mut session.settings.passkeys;
            passkeys.finish_remove(&id);
            let list = std::mem::take(&mut passkeys.list);
            passkeys.accept_list(list);
        }
        cx.notify();
    }

    /// A passkey's software icon, animated once decoded; `None` until the
    /// custom emoji is resolved and downloaded.
    fn passkey_emoji(
        &self,
        custom_emoji_id: i64,
        size: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if custom_emoji_id == 0 {
            return None;
        }
        let session = self.session()?;
        let item = session
            .stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .find(|item| item.custom_emoji_id == Some(custom_emoji_id))?;
        let source = (!power_saving::on(power_saving::Flag::EmojiChat))
            .then(|| self.custom_emoji_image(item.file_id, item.format, cx))
            .flatten()
            .map(ImageSource::from)
            .or_else(|| {
                let roots = self.media_display_roots();
                item.display_file_id()
                    .and_then(|file| session.media.files.get(&file.0))
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .map(ImageSource::from)
            })?;
        Some(
            img(source)
                .size(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        )
    }

    fn passkey_row(&self, passkey: &Passkey, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let icon = self
            .passkey_emoji(passkey.software_icon_custom_emoji_id, ICON_SIZE, cx)
            .unwrap_or_else(|| {
                Icon::new(IconName::UserKey)
                    .size(px(24.))
                    .text_color(cx.theme().primary)
                    .into_any_element()
            });
        let now = civil_local(now_unix());
        let owner = cx.entity().downgrade();
        let id = passkey.id.clone();
        let title = passkey.title().to_string();
        div()
            .id(SharedString::from(format!("passkey-{}", passkey.id)))
            .test_support()
            .role(Role::ListItem)
            .aria_label(title.clone())
            .flex()
            .items_center()
            .gap_3()
            .py_1p5()
            .child(
                div()
                    .size(px(ICON_SIZE))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .truncate()
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(passkey.date_line(civil_local, &now)),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("passkey-menu-{}", passkey.id)))
                    .icon(IconName::EllipsisVertical)
                    .ghost()
                    .small()
                    .disabled(busy)
                    .tooltip(tr(keys::Q_MEDIA_OPTIONS))
                    .accessibility_label(format!("{title}, {}", tr(keys::Q_MEDIA_OPTIONS)))
                    .dropdown_menu(move |menu, _, _| {
                        let owner = owner.clone();
                        let id = id.clone();
                        menu.item(
                            PopupMenuItem::new(tr(keys::BOX_DELETE))
                                .icon(IconName::Trash)
                                .on_click(move |_, _, cx| {
                                    let id = id.clone();
                                    let _ = owner
                                        .update(cx, |this, cx| this.ask_delete_passkey(id, cx));
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    fn build_passkeys_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Passkeys, |this, _, cx| {
                this.close_passkeys(cx);
            });
        app.update(cx, |this, cx| {
            let muted = cx.theme().muted_foreground;
            let state = this.passkeys_state().cloned().unwrap_or_default();
            let busy = state.removing.is_some();
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(header_icon(IconName::UserKey, cx))
                .child(
                    div()
                        .text_sm()
                        .text_center()
                        .text_color(muted)
                        .child(tr(ABOUT)),
                );
            // A confirmation whose passkey is gone (removed) is done.
            if let Some(id) = this.privacy.passkeys.confirm.as_deref()
                && state.find(id).is_none()
            {
                this.privacy.passkeys.confirm = None;
            }
            if let Some(error) = state.error.clone() {
                body = body.child(
                    div()
                        .id("passkeys-error")
                        .role(Role::Status)
                        .text_xs()
                        .text_color(danger())
                        .child(error),
                );
            }
            let mut list = div()
                .id("passkeys-list")
                .role(Role::List)
                .aria_label(tr(TITLE))
                .flex()
                .flex_col();
            for passkey in &state.list {
                list = list.child(this.passkey_row(passkey, busy, cx));
            }
            body = body.child(list).when(state.can_register(), |body| {
                body.child(
                    Button::new("passkeys-add")
                        .label(tr(keys::SETTINGS_PASSKEYS_BUTTON))
                        .icon(IconName::Plus)
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.open_passkeys_about(cx))),
                )
            });
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(BUTTON_ABOUT))
                    .child(
                        Button::new("passkeys-learn-more")
                            .label(tr(keys::Q_PREFS_LEARN_MORE))
                            .link()
                            .xsmall()
                            .on_click(cx.listener(|this, _, _, cx| this.open_passkeys_about(cx))),
                    ),
            );
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(tr(TITLE)))
                .width(px(440.))
                .content(crate::ui::shell::scrollable_dialog_content(once(body)))
                .on_close(on_close)
        })
    }

    fn build_passkey_delete_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PasskeyDelete, |this, _, cx| {
                this.dismiss_delete_passkey(cx);
            });
        app.update(cx, |this, cx| {
            let state = this.passkeys_state().cloned().unwrap_or_default();
            let passkey = this
                .privacy
                .passkeys
                .confirm
                .as_deref()
                .and_then(|id| state.find(id))
                .cloned();
            let busy = state.removing.is_some();
            let mut body = div().flex().flex_col().gap_3();
            if let Some(passkey) = &passkey {
                let emoji = this.passkey_emoji(passkey.software_icon_custom_emoji_id, 18., cx);
                body = body.child(question(&passkey.name, emoji));
            }
            if let Some(error) = state.error.clone() {
                body = body.child(
                    div()
                        .id("passkey-delete-error")
                        .role(Role::Status)
                        .text_xs()
                        .text_color(danger())
                        .child(error),
                );
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("passkey-delete-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_delete_passkey(cx);
                            this.close_kit_dialog_if_done(DialogKind::PasskeyDelete, window, cx);
                        })),
                )
                .child(
                    Button::new("passkey-delete-confirm")
                        .label(tr(keys::BOX_DELETE))
                        .danger()
                        .loading(busy)
                        .disabled(busy || passkey.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_delete_passkey(cx))),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr(
                    keys::SETTINGS_PASSKEYS_DELETE_SURE_TITLE,
                )))
                .width(px(400.))
                .content(crate::ui::shell::scrollable_dialog_content(once(
                    body.into_any_element(),
                )))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn build_passkeys_about_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PasskeysAbout, |this, _, cx| {
                this.privacy.passkeys.about_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let muted = cx.theme().muted_foreground;
            let can_register = this
                .passkeys_state()
                .is_some_and(PasskeysState::can_register);
            let busy = this.passkey_creation_busy();
            let error = this.passkeys_state().and_then(|s| s.error.clone());
            let entry = |icon: IconName, title: &'static str, about: &'static str| {
                div()
                    .flex()
                    .gap_3()
                    .child(Icon::new(icon).size(px(20.)).flex_none().mt_0p5())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child(title))
                            .child(div().text_sm().text_color(muted).child(about)),
                    )
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_4()
                .child(header_icon(IconName::UserKey, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id("passkeys-about-title")
                                .role(Role::Heading)
                                .text_lg()
                                .font_semibold()
                                .child(tr(keys::SETTINGS_PASSKEYS_NONE_TITLE)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_center()
                                .text_color(muted)
                                .child(tr(keys::SETTINGS_PASSKEYS_NONE_ABOUT)),
                        ),
                )
                .child(entry(
                    IconName::UserKey,
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO1_TITLE),
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO1_ABOUT),
                ))
                .child(entry(
                    IconName::ScanFace,
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO2_TITLE),
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO2_ABOUT),
                ))
                .child(entry(
                    IconName::Lock,
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO3_TITLE),
                    tr(keys::SETTINGS_PASSKEYS_NONE_INFO3_ABOUT),
                ))
                .when_some(error, |body, error| {
                    body.child(
                        div()
                            .id("passkeys-about-error")
                            .test_support()
                            .role(Role::Alert)
                            .aria_label(error.clone())
                            .text_sm()
                            .text_color(danger())
                            .child(error),
                    )
                });
            // Without WebAuthn the button reads "Unsupported" and does
            // nothing (tdesktop's own state when it can't register).
            let button = Button::new("passkeys-create")
                .label(tr(if can_register {
                    keys::SETTINGS_PASSKEYS_NONE_BUTTON
                } else {
                    keys::SETTINGS_PASSKEYS_NONE_BUTTON_UNSUPPORTED
                }))
                .primary()
                .w_full()
                .disabled(!can_register)
                .loading(busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.start_passkey_creation(window, cx);
                }));
            dialog
                .width(px(440.))
                .content(crate::ui::shell::scrollable_dialog_content(once(
                    body.into_any_element(),
                )))
                .footer(div().w_full().child(button))
                .on_close(on_close)
        })
    }
}

/// `content` needs an `Fn` closure, but the body is built once per dialog
/// render: hand it over through a one-shot cell.
pub(super) fn once(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static {
    let body = std::rc::Rc::new(std::cell::RefCell::new(Some(body)));
    move |content, _, _| {
        let body = body
            .borrow_mut()
            .take()
            .unwrap_or_else(|| div().into_any_element());
        content.child(body)
    }
}

/// The round icon at the top of the list and the box (tdesktop plays the
/// `passkeys` Lottie there).
pub(super) fn header_icon(icon: IconName, cx: &App) -> impl IntoElement {
    div().flex().justify_center().child(
        div()
            .size(px(72.))
            .rounded_full()
            .bg(cx.theme().primary.opacity(0.12))
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(icon).size(px(32.)).text_color(cx.theme().primary)),
    )
}

/// "Once deleted, {emoji} **name** can't be used to log in." and the
/// reminder, wrapping word by word so the emoji and the bold name stay
/// inline.
fn question(name: &str, emoji: Option<AnyElement>) -> impl IntoElement {
    let (before, bold, after) = delete_question(name);
    let (first, rest) = after.split_once("\n\n").unwrap_or((after.as_str(), ""));
    let words = |text: &str| -> Vec<AnyElement> {
        text.split_inclusive(' ')
            .map(|word| {
                div()
                    .whitespace_nowrap()
                    .child(word.to_string())
                    .into_any_element()
            })
            .collect()
    };
    let spoken = format!("{before}{}{first}", bold.as_deref().unwrap_or_default());
    let mut line = div()
        .id("passkey-delete-question")
        .test_support()
        .role(Role::Paragraph)
        .aria_label(spoken)
        .flex()
        .flex_wrap()
        .items_center()
        .text_sm()
        .children(words(before));
    if let Some(bold) = bold {
        if let Some(emoji) = emoji {
            line = line.child(div().mr_1().child(emoji));
        }
        line = line.child(div().font_semibold().child(bold));
    }
    line = line.children(words(first));
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(line)
        .child(div().text_sm().child(rest.to_string()))
}

crate::ui::shell::register_dialogs! {
    /// The delete confirmation over the Passkeys list.
    PasskeyDelete => DialogSpec::new(
        // Before the list, so it shows instead of it and closes back to it.
        610,
        |app| app.privacy.passkeys.confirm.as_deref().is_some_and(|id| {
            // Closes once the server confirmed the removal.
            app.session()
                .is_some_and(|s| s.settings.passkeys.find(id).is_some())
        }),
        QuillApp::build_passkey_delete_dialog,
    )
    .closes(|app, _, cx| app.dismiss_delete_passkey(cx)),

    /// "Protect your account" (`PasskeysNoneBox`).
    PasskeysAbout => DialogSpec::new(
        620,
        |app| app.privacy.passkeys.about_open,
        QuillApp::build_passkeys_about_dialog,
    ),

    /// The Passkeys list.
    Passkeys => DialogSpec::new(
        630,
        |app| app.privacy.passkeys.list_open,
        QuillApp::build_passkeys_dialog,
    )
    .closes(|app, _, cx| app.close_passkeys(cx)),
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-passkeys")
        .setup(QuillApp::demo_passkeys),
];

/// Two passkeys for the demo: one used yesterday with an app icon id the
/// demo cannot resolve (the fallback icon shows), one never used.
pub(in crate::ui) fn demo_passkeys_list() -> Vec<Passkey> {
    let now = now_unix();
    vec![
        Passkey {
            id: "demo-passkey-icloud".into(),
            name: tr(keys::Q_PREFS_ICLOUD_KEYCHAIN).into(),
            addition_date: now.saturating_sub(40 * 86_400),
            last_usage_date: now.saturating_sub(86_400),
            software_icon_custom_emoji_id: 5_368_324_170_671_202_286,
        },
        Passkey {
            id: "demo-passkey-yubikey".into(),
            name: tr(keys::Q_PREFS_YUBIKEY_5C).into(),
            addition_date: now.saturating_sub(12 * 86_400),
            last_usage_date: 0,
            software_icon_custom_emoji_id: 0,
        },
    ]
}

impl QuillApp {
    /// The demo account's passkeys, allowed by the (demo) app config.
    pub(in crate::ui) fn seed_demo_passkeys(&mut self) {
        if let Some(session) = self.demo_session.as_mut() {
            let passkeys = &mut session.settings.passkeys;
            passkeys.display_allowed = true;
            passkeys.accept_list(demo_passkeys_list());
        }
    }

    /// `ready-passkeys`: the list, or with `QUILL_DEMO_PASSKEYS` set to
    /// `privacy`, `about` or `delete`, the Privacy and Security screen,
    /// the "Protect your account" box or the delete confirmation.
    fn demo_passkeys(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.seed_demo_passkeys();
        match std::env::var("QUILL_DEMO_PASSKEYS")
            .unwrap_or_default()
            .as_str()
        {
            "privacy" => {
                self.open_settings(Some(quill::settings_catalog::Page::Privacy));
            }
            "about" => {
                self.open_passkeys(cx);
                self.open_passkeys_about(cx);
            }
            "delete" => {
                self.open_passkeys(cx);
                self.ask_delete_passkey("demo-passkey-yubikey".into(), cx);
            }
            _ => self.open_passkeys(cx),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "passkeys_tests.rs"]
mod tests;

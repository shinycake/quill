//! Mini apps, the window side (docs/decisions/codex-miniapp-webview.md).
//!
//! Every mini app runs in `quill-webview`, a helper process next to the
//! Quill executable that owns the system web view. This module starts it,
//! feeds it commands over its stdin and reads its events from its stdout
//! (`quill-webview-protocol`), and is the only place that decides what a
//! bridge event may do: links go through the link policy, consent prompts
//! are shown and answered here, TDLib is called from here.
//!
//! Telegram Desktop's behaviour is the model (`bot_attach_web_view.cpp`,
//! `attach_bot_webview.cpp`): the first open of a bot's app asks once
//! ("By launching this mini app, you agree to..."), adding an attachment
//! menu bot asks with an optional "allow messages" checkbox, and the
//! bridge's write-access and clipboard requests ask in a popup over the
//! app. Payments (`web_app_open_invoice`) are out of scope and refused.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::link_policy::{OpenDecision, open_decision};
use quill::state::{DeepLinkAction, WebAppOpenResult, WriteAccessResult};
use quill::telegram::envelope::{AttachChatKind, AttachmentMenuBot, ChatKind, MessageSender};
use quill::text::LinkTarget;
use quill::web_app::bridge::{BridgeReply, ColorSpec, WebAppEvent, parse_event};
use quill::web_app::theme::{Rgb, ThemeParams};
use quill::web_app::{LaunchSource, MINI_APP_TERMS_URL, WebAppLaunch, menu_items, trust};
use quill_webview_protocol::{
    BottomButton, HelperEvent, HostCommand, MenuItem, Popup, PopupButton, ShellEvent, decode,
    encode,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

/// What a launch needs before TDLib is asked, kept while a box is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingOpen {
    /// `openWebApp` in `chat_id` (menu button, inline button).
    Open {
        chat_id: ChatId,
        bot_id: i64,
        url: String,
        source: LaunchSource,
    },
    /// `getWebAppUrl` (keyboard button).
    Simple {
        chat_id: ChatId,
        bot_id: i64,
        url: String,
        button_text: String,
    },
    /// `getMainWebApp`.
    Main {
        chat_id: Option<ChatId>,
        bot_id: i64,
        start_parameter: String,
    },
    /// `getWebAppLinkUrl` (`t.me/bot/app`).
    Link {
        chat_id: Option<ChatId>,
        bot_id: i64,
        short_name: String,
        start_parameter: String,
    },
    /// `openWebApp` with an empty URL (attachment menu).
    Attach { chat_id: ChatId, bot_id: i64 },
}

impl PendingOpen {
    fn bot_id(&self) -> i64 {
        match self {
            Self::Open { bot_id, .. }
            | Self::Simple { bot_id, .. }
            | Self::Main { bot_id, .. }
            | Self::Link { bot_id, .. }
            | Self::Attach { bot_id, .. } => *bot_id,
        }
    }
}

/// The box in the main window (`DialogKind::WebAppConfirm`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WebAppConfirm {
    /// First open of a bot's app (tdesktop `confirmOpen`).
    OpenTerms {
        then: PendingOpen,
        /// The bot asks to message the person (named app links).
        request_write: bool,
        allow_write: bool,
    },
    /// The bot asks to be added to the attachment menu (tdesktop
    /// `requestAddToMenu`).
    AddToMenu {
        bot: AttachmentMenuBot,
        allow_write: bool,
        then: PendingOpen,
    },
}

/// What a shell popup was for, so its answer can be acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PopupPurpose {
    /// `web_app_open_popup`: the app gets `popup_closed`.
    App,
    /// "Allow messaging": yes calls `allowBotToSendMessages`.
    WriteAccess,
    /// "Paste from clipboard": yes reads the clipboard for the app.
    Clipboard { req_id: String },
    /// "Close anyway" after `web_app_setup_closing_behavior`.
    CloseConfirm,
}

/// Something only a render (with the `Window`) can do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RenderAction {
    /// `web_app_switch_inline_query` into the current chat's composer.
    SwitchInline { query: String },
    /// The menu's "Open bot".
    OpenBotChat { bot_id: i64 },
}

/// The helper process and the app it shows.
pub(super) struct MiniAppWindow {
    child: Child,
    stdin: ChildStdin,
    events: mpsc::Receiver<HelperEvent>,
    launch: WebAppLaunch,
    theme: ThemeParams,
    ready: bool,
    closing_confirmation: bool,
    settings_button: bool,
    popups: HashMap<String, PopupPurpose>,
    popup_seq: u64,
    waiting_write_access: bool,
    data_sent: bool,
    privacy_policy_url: Option<String>,
    in_attachment_menu: bool,
}

impl Drop for MiniAppWindow {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(encode(&HostCommand::Close).as_bytes());
        let _ = self.stdin.flush();
        // Give it a moment to exit on its own, then make sure.
        for _ in 0..20 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl MiniAppWindow {
    fn send(&mut self, command: &HostCommand) {
        let _ = self.stdin.write_all(encode(command).as_bytes());
        let _ = self.stdin.flush();
    }

    fn emit(&mut self, reply: BridgeReply) {
        self.send(&HostCommand::Emit {
            event: reply.event,
            data: reply.data,
        });
    }

    fn show_popup(&mut self, purpose: PopupPurpose, popup_without_id: Popup) {
        self.popup_seq += 1;
        let id = format!("p{}", self.popup_seq);
        self.popups.insert(id.clone(), purpose);
        self.send(&HostCommand::ShowPopup {
            popup: Popup {
                id,
                ..popup_without_id
            },
        });
    }

    fn menu(&self) -> Vec<MenuItem> {
        menu_items(
            self.settings_button,
            self.privacy_policy_url.is_some(),
            self.in_attachment_menu,
        )
        .into_iter()
        .map(|(id, label, attention)| MenuItem {
            id: id.to_string(),
            label: label.to_string(),
            attention,
        })
        .collect()
    }

    /// `#rrggbb` for a bridge color, or the theme's color for a key.
    fn resolve_color(&self, spec: &ColorSpec) -> String {
        match spec {
            ColorSpec::Rgb(hex) => hex.clone(),
            ColorSpec::Key(key) => self.theme.key_color(*key).hex(),
        }
    }
}

/// Everything mini-app related on [`QuillApp`].
#[derive(Default)]
pub(super) struct MiniApps {
    pub(super) window: Option<MiniAppWindow>,
    pub(super) confirm: Option<WebAppConfirm>,
    /// A launch waiting for `getAttachmentMenuBot` (deep link) before it
    /// can go on.
    awaiting_attach_bot: Option<PendingOpen>,
    /// A launch waiting for `searchWebApp` before the open box.
    awaiting_search: Option<PendingOpen>,
    /// A launch waiting for `toggleBotIsAddedToAttachmentMenu`.
    awaiting_toggle: Option<PendingOpen>,
    render_action: Option<RenderAction>,
    /// The poll task, alive while a window is.
    poll: Option<Task<()>>,
    generation: u64,
}

/// Where the helper executable is: next to Quill's own (the app bundle's
/// `Contents/MacOS`, the Linux and Windows package directories, and
/// `target/<profile>` for developer runs).
pub(super) fn helper_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "quill-webview.exe"
    } else {
        "quill-webview"
    };
    let sibling = exe.with_file_name(name);
    sibling.is_file().then_some(sibling)
}

/// The web view's storage for one bot of one account: cookies and local
/// storage never cross bots (tdesktop keeps a `webview` folder per bot).
/// WebView2 and WebKitGTK take a directory; WKWebView takes a 128-bit
/// data-store identifier ([`data_store_id`]), so both are passed.
fn data_dir(account: &str, bot_id: i64) -> Option<std::path::PathBuf> {
    let root = quill::settings::safe_app_root()?;
    Some(
        root.join("webview")
            .join(account)
            .join(format!("bot-{bot_id}")),
    )
}

/// 16 bytes (as hex) naming the WebKit data store of one bot of one
/// account: a hash, so the id carries nothing readable.
fn data_store_id(account: &str, bot_id: i64) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("quill-webview/{account}/{bot_id}").as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hsla_rgb(color: Hsla) -> Rgb {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

/// The kit theme as the mini app sees it.
pub(super) fn theme_params(cx: &App) -> ThemeParams {
    let theme = cx.theme();
    ThemeParams {
        background: hsla_rgb(theme.background),
        secondary_background: hsla_rgb(theme.sidebar),
        header_background: hsla_rgb(theme.background),
        bottom_bar_background: hsla_rgb(theme.background),
        section_background: hsla_rgb(theme.background),
        section_separator: hsla_rgb(theme.border),
        text: hsla_rgb(theme.foreground),
        accent_text: hsla_rgb(theme.primary),
        section_header_text: hsla_rgb(theme.muted_foreground),
        subtitle_text: hsla_rgb(theme.muted_foreground),
        destructive_text: hsla_rgb(theme.danger),
        hint: hsla_rgb(theme.muted_foreground),
        link: hsla_rgb(theme.link),
        button: hsla_rgb(theme.primary),
        button_text: hsla_rgb(theme.primary_foreground),
        dark: theme.is_dark(),
    }
}

fn popup(title: &str, message: String, buttons: Vec<(&str, &str, &str)>) -> Popup {
    Popup {
        id: String::new(),
        title: title.to_string(),
        message,
        buttons: buttons
            .into_iter()
            .map(|(id, text, kind)| PopupButton {
                id: id.to_string(),
                text: text.to_string(),
                kind: kind.to_string(),
            })
            .collect(),
    }
}

/// Whether `url` is one of Telegram's own that the deep-link handler
/// resolves (`tg:` or a `t.me` link).
fn is_telegram_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("tg:")
        || quill::link_policy::host_of(&lower)
            .is_some_and(|host| host == "t.me" || host == "telegram.me" || host == "telegram.dog")
}

impl QuillApp {
    // Launch points ----------------------------------------------------------

    /// The bot's menu button (`botMenuButton.url`).
    pub(super) fn open_bot_menu_web_app(&mut self, url: &str, cx: &mut Context<Self>) {
        let Some((chat_id, bot_id)) = self.session().and_then(|session| {
            let chat_id = session.open_chat?;
            Some((chat_id, session.bot_user_id_for_chat(chat_id)?))
        }) else {
            self.set_status_note("Open the bot's chat to use its menu.", cx);
            return;
        };
        self.start_web_app(
            PendingOpen::Open {
                chat_id,
                bot_id,
                url: url.to_string(),
                source: LaunchSource::MenuButton,
            },
            cx,
        );
    }

    /// An inline keyboard `web_app` button: the app belongs to the
    /// message's sender bot.
    pub(super) fn open_inline_web_app(
        &mut self,
        chat_id: ChatId,
        message_id: quill::ids::MessageId,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        let bot_id = self.session().and_then(|session| {
            let message = session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.get(&message_id.0))?;
            match message.sender {
                Some(MessageSender::User { user_id })
                    if session.users.get(&user_id).is_some_and(|user| user.is_bot) =>
                {
                    Some(user_id)
                }
                _ => session.bot_user_id_for_chat(chat_id),
            }
        });
        let Some(bot_id) = bot_id else {
            self.set_status_note("Only a bot can open a mini app.", cx);
            return;
        };
        self.start_web_app(
            PendingOpen::Open {
                chat_id,
                bot_id,
                url: url.to_string(),
                source: LaunchSource::InlineButton,
            },
            cx,
        );
    }

    /// A custom keyboard `web_app` button ("simple" app).
    pub(super) fn open_simple_web_app(
        &mut self,
        chat_id: ChatId,
        button_text: &str,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(bot_id) = self
            .session()
            .and_then(|session| session.bot_user_id_for_chat(chat_id))
        else {
            self.set_status_note("Only a bot can open a mini app.", cx);
            return;
        };
        self.start_web_app(
            PendingOpen::Simple {
                chat_id,
                bot_id,
                url: url.to_string(),
                button_text: button_text.to_string(),
            },
            cx,
        );
    }

    /// The profile's "Open App", the Apps tab, `t.me/bot?startapp`.
    pub(super) fn open_main_web_app(
        &mut self,
        bot_id: i64,
        start_parameter: &str,
        cx: &mut Context<Self>,
    ) {
        let chat_id = self.private_chat_with(bot_id);
        self.start_web_app(
            PendingOpen::Main {
                chat_id,
                bot_id,
                start_parameter: start_parameter.to_string(),
            },
            cx,
        );
    }

    /// An attachment menu bot from the paperclip menu, in the open chat.
    pub(super) fn open_attachment_menu_bot(&mut self, bot_id: i64, cx: &mut Context<Self>) {
        let Some(chat_id) = self.session().and_then(|session| session.open_chat) else {
            return;
        };
        self.start_web_app(PendingOpen::Attach { chat_id, bot_id }, cx);
    }

    /// The attachment menu bots that can open in the current chat.
    pub(super) fn attachment_menu_bots_for_open_chat(&self) -> Vec<(i64, String)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let Some(chat_id) = session.open_chat else {
            return Vec::new();
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return Vec::new();
        };
        let kind = match &chat.kind {
            ChatKind::Private { user_id } if Some(user_id.0) == session.my_user_id => {
                AttachChatKind::SelfChat
            }
            ChatKind::Private { .. } if session.bot_user_id_for_chat(chat_id).is_some() => {
                AttachChatKind::Bot
            }
            ChatKind::Private { .. } => AttachChatKind::User,
            ChatKind::Supergroup {
                is_channel: true, ..
            } => AttachChatKind::Channel,
            ChatKind::Secret { .. } => return Vec::new(),
            _ => AttachChatKind::Group,
        };
        session
            .bots
            .web_apps
            .attachment_menu_bots
            .iter()
            .filter(|bot| bot.is_added && bot.show_in_attachment_menu)
            .filter(|bot| {
                // In the bot's own chat the bot's own flag decides.
                if kind == AttachChatKind::Bot
                    && session.bot_user_id_for_chat(chat_id) == Some(bot.bot_user_id)
                {
                    bot.supports_self_chat
                } else {
                    bot.supports_chat(kind)
                }
            })
            .map(|bot| (bot.bot_user_id, bot.name.clone()))
            .collect()
    }

    /// A resolved `t.me/bot/app`, `t.me/bot?startapp` or `?startattach`
    /// link (the bot's private chat is `chat_id`).
    pub(super) fn run_web_app_link(
        &mut self,
        chat_id: ChatId,
        bot_id: i64,
        action: &DeepLinkAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            DeepLinkAction::OpenWebAppLink {
                short_name,
                start_parameter,
                ..
            } => {
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.search_web_app(bot_id, short_name));
                if matches!(sent, Some(Ok(_))) {
                    self.mini_apps.awaiting_search = Some(PendingOpen::Link {
                        chat_id: Some(chat_id),
                        bot_id,
                        short_name: short_name.clone(),
                        start_parameter: start_parameter.clone(),
                    });
                    self.ensure_mini_app_poll(cx);
                } else {
                    self.set_status_note("Couldn't look up that app.", cx);
                }
            }
            DeepLinkAction::OpenMainWebApp {
                start_parameter, ..
            } => {
                self.start_web_app(
                    PendingOpen::Main {
                        chat_id: Some(chat_id),
                        bot_id,
                        start_parameter: start_parameter.clone(),
                    },
                    cx,
                );
            }
            DeepLinkAction::OpenAttachmentBot { .. } => {
                // The current chat, as tdesktop's `targetChatCurrent`.
                let target = self
                    .session()
                    .and_then(|session| session.open_chat)
                    .unwrap_or(chat_id);
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.fetch_attachment_menu_bot(bot_id));
                if matches!(sent, Some(Ok(_))) {
                    self.mini_apps.awaiting_attach_bot = Some(PendingOpen::Attach {
                        chat_id: target,
                        bot_id,
                    });
                    self.ensure_mini_app_poll(cx);
                } else {
                    self.set_status_note("This bot can't be added to the attachment menu.", cx);
                }
            }
            _ => return false,
        }
        true
    }

    fn private_chat_with(&self, user_id: i64) -> Option<ChatId> {
        self.session().and_then(|session| {
            session
                .chats
                .values()
                .find(|chat| matches!(&chat.kind, ChatKind::Private { user_id: peer } if peer.0 == user_id))
                .map(|chat| chat.id)
        })
    }

    // The open box ------------------------------------------------------------

    /// Ask first when tdesktop would (`confirmOpen`), then call TDLib.
    fn start_web_app(&mut self, open: PendingOpen, cx: &mut Context<Self>) {
        if self.live.is_none() {
            self.set_status_note("Mini apps need a Telegram connection.", cx);
            return;
        }
        if helper_path().is_none() {
            self.set_status_note(
                "The mini app window (quill-webview) is missing next to Quill.",
                cx,
            );
            return;
        }
        let bot_id = open.bot_id();
        let verified = self
            .session()
            .and_then(|session| session.users.get(&bot_id))
            .is_some_and(|user| user.verification.is_verified);
        if trust::needs_confirmation(bot_id, verified, false) {
            self.mini_apps.confirm = Some(WebAppConfirm::OpenTerms {
                then: open,
                request_write: false,
                allow_write: false,
            });
            cx.notify();
            return;
        }
        self.request_web_app(open, false, cx);
    }

    /// The TDLib call for a launch.
    fn request_web_app(&mut self, open: PendingOpen, allow_write: bool, cx: &mut Context<Self>) {
        // One app at a time: a new launch replaces the open window.
        self.close_mini_app_window(cx);
        let theme = theme_params(cx);
        let sent = self.live.as_mut().map(|live| match &open {
            PendingOpen::Open {
                chat_id,
                bot_id,
                url,
                source,
            } => live
                .driver
                .open_web_app(*chat_id, *bot_id, url, source.clone(), &theme)
                .map(|_| ()),
            PendingOpen::Simple {
                chat_id,
                bot_id,
                url,
                button_text,
            } => live
                .driver
                .get_web_app_url(*chat_id, *bot_id, url, button_text, &theme)
                .map(|_| ()),
            PendingOpen::Main {
                chat_id,
                bot_id,
                start_parameter,
            } => live
                .driver
                .get_main_web_app(
                    chat_id.unwrap_or(ChatId(0)),
                    *bot_id,
                    start_parameter,
                    &theme,
                )
                .map(|_| ()),
            PendingOpen::Link {
                chat_id,
                bot_id,
                short_name,
                start_parameter,
            } => live
                .driver
                .get_web_app_link_url(
                    chat_id.unwrap_or(ChatId(0)),
                    *bot_id,
                    short_name,
                    start_parameter,
                    allow_write,
                    &theme,
                )
                .map(|_| ()),
            PendingOpen::Attach { chat_id, bot_id } => live
                .driver
                .open_web_app(*chat_id, *bot_id, "", LaunchSource::AttachmentMenu, &theme)
                .map(|_| ()),
        });
        if matches!(sent, Some(Ok(()))) {
            self.set_status_note("Opening the mini app…", cx);
            self.ensure_mini_app_poll(cx);
        } else {
            self.set_status_note("Couldn't open the mini app.", cx);
        }
    }

    /// The box's main button.
    pub(super) fn accept_web_app_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.mini_apps.confirm.take() else {
            return;
        };
        match confirm {
            WebAppConfirm::OpenTerms {
                then,
                request_write,
                allow_write,
            } => {
                trust::mark_trusted(then.bot_id());
                self.request_web_app(then, request_write && allow_write, cx);
            }
            WebAppConfirm::AddToMenu {
                bot,
                allow_write,
                then,
            } => {
                let sent = self.live.as_mut().map(|live| {
                    live.driver.toggle_bot_in_attachment_menu(
                        bot.bot_user_id,
                        true,
                        bot.request_write_access && allow_write,
                    )
                });
                if matches!(sent, Some(Ok(_))) {
                    self.mini_apps.awaiting_toggle = Some(then);
                    self.ensure_mini_app_poll(cx);
                } else {
                    self.set_status_note("Couldn't add the bot to the menu.", cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_web_app_confirm(&mut self, cx: &mut Context<Self>) {
        self.mini_apps.confirm = None;
        cx.notify();
    }

    pub(super) fn set_web_app_confirm_write(&mut self, on: bool, cx: &mut Context<Self>) {
        match &mut self.mini_apps.confirm {
            Some(WebAppConfirm::OpenTerms { allow_write, .. })
            | Some(WebAppConfirm::AddToMenu { allow_write, .. }) => *allow_write = on,
            None => {}
        }
        cx.notify();
    }

    pub(super) fn build_web_app_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::WebAppConfirm, |this, _, cx| {
                this.mini_apps.confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let muted = cx.theme().muted_foreground;
            let Some(confirm) = this.mini_apps.confirm.clone() else {
                return dialog;
            };
            let bot_name = |this: &QuillApp, bot_id: i64| {
                this.session()
                    .and_then(|session| session.users.get(&bot_id))
                    .map(|user| user.display_name())
                    .unwrap_or_else(|| "the bot".into())
            };
            let (title, lines, checkbox, action): (String, Vec<String>, Option<(bool, String)>, &str) =
                match &confirm {
                    WebAppConfirm::OpenTerms {
                        then,
                        request_write,
                        allow_write,
                    } => {
                        let name = bot_name(this, then.bot_id());
                        (
                            format!("Open {name}'s mini app?"),
                            vec![
                                "By launching this mini app, you agree to the Terms of Service for Mini Apps.".into(),
                                "It will connect to the bot's website, which will see your IP address and basic device information.".into(),
                            ],
                            request_write.then(|| (*allow_write, format!("Allow {name} to send me messages"))),
                            "Open",
                        )
                    }
                    WebAppConfirm::AddToMenu {
                        bot, allow_write, ..
                    } => {
                        let name = if bot.name.is_empty() {
                            bot_name(this, bot.bot_user_id)
                        } else {
                            bot.name.clone()
                        };
                        (
                            format!("Add {name} to the attachment menu?"),
                            vec![format!(
                                "{name} asks to be added as an option to your attachment menu, so you can open it from any chat."
                            )],
                            bot.request_write_access
                                .then(|| (*allow_write, format!("Allow {name} to send me messages"))),
                            "Add",
                        )
                    }
                };
            let mut body = div().flex().flex_col().gap_3();
            for line in lines {
                body = body.child(div().text_sm().text_color(muted).child(line));
            }
            if matches!(confirm, WebAppConfirm::OpenTerms { .. }) {
                body = body.child(
                    Button::new("web-app-terms")
                        .label("Terms of Service for Mini Apps")
                        .icon(IconName::ExternalLink)
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_message_url(MINI_APP_TERMS_URL, cx);
                        })),
                );
            }
            if let Some((on, label)) = checkbox {
                body = body.child(
                    Checkbox::new("web-app-allow-write")
                        .checked(on)
                        .label(label)
                        .on_click(cx.listener(|this, &on: &bool, window, cx| {
                            this.set_web_app_confirm_write(on, cx);
                            window.refresh();
                        })),
                );
            }
            let body = body.into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("web-app-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_web_app_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::WebAppConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("web-app-accept")
                        .label(action)
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.accept_web_app_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::WebAppConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(super::shell::dialog_title(title))
                .content(super::shell::scrollable_dialog_content({
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

    // The window ----------------------------------------------------------------
}

fn bottom_button(setup: quill::web_app::bridge::ButtonSetup) -> BottomButton {
    BottomButton {
        visible: setup.visible,
        active: setup.active,
        text: setup.text,
        color: setup.color.unwrap_or_default(),
        text_color: setup.text_color.unwrap_or_default(),
        progress: setup.progress,
        shine: setup.shine,
        position: setup.position,
    }
}

mod spawn_mini_app_window;

#[cfg(test)]
mod tests {
    // No `super::*`: the kit's glob brings its own `test` attribute.
    use super::{data_store_id, hsla_rgb, is_telegram_link, popup};
    use quill::web_app::theme::Rgb;

    #[test]
    fn data_store_ids_are_stable_per_account_and_bot() {
        assert_eq!(data_store_id("acc", 42), data_store_id("acc", 42));
        assert_ne!(data_store_id("acc", 42), data_store_id("acc", 43));
        assert_ne!(data_store_id("acc", 42), data_store_id("other", 42));
        assert_eq!(data_store_id("acc", 42).len(), 32);
    }

    #[test]
    fn telegram_links_are_told_apart_from_the_web() {
        assert!(is_telegram_link("tg://resolve?domain=x"));
        assert!(is_telegram_link("https://t.me/durov"));
        assert!(is_telegram_link("HTTPS://T.ME/+abc"));
        assert!(!is_telegram_link("https://example.com/t.me"));
        assert!(!is_telegram_link("https://evil.t.me.example/"));
    }

    #[test]
    fn popup_helper_keeps_button_order() {
        let popup = popup(
            "T",
            "M".into(),
            vec![
                ("cancel", "Cancel", "cancel"),
                ("allow", "Allow", "default"),
            ],
        );
        assert_eq!(popup.buttons.len(), 2);
        assert_eq!(popup.buttons[1].id, "allow");
        assert!(popup.id.is_empty(), "the window assigns ids");
    }

    #[test]
    fn colors_convert_to_rgb_bytes() {
        assert_eq!(hsla_rgb(gpui_kit::white()), Rgb(255, 255, 255));
        assert_eq!(hsla_rgb(gpui_kit::black()), Rgb(0, 0, 0));
    }
}

crate::ui::shell::register_dialogs! {
    /// Mini apps: the first-open terms box / add to the attachment menu.
    WebAppConfirm => DialogSpec::new(
        2550,
        |app| app.mini_apps.confirm.is_some(),
        QuillApp::build_web_app_confirm_dialog,
    ),
}

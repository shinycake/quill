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

    /// Demo: open the helper on a local page without TDLib
    /// (`QUILL_DEMO_MINIAPP=window`, `QUILL_DEMO_MINIAPP_URL`).
    pub(super) fn open_mini_app_window_demo(&mut self, url: &str, cx: &mut Context<Self>) {
        self.spawn_mini_app_window(
            WebAppLaunch {
                bot_user_id: 21,
                bot_name: "Weather Desk".into(),
                chat_id: Some(21),
                source: LaunchSource::InlineButton,
                launch_id: 0,
                url: url.to_string(),
            },
            cx,
        );
    }

    /// Demo and tests: put a box up without a connection.
    pub(super) fn show_web_app_confirm_demo(
        &mut self,
        confirm: WebAppConfirm,
        cx: &mut Context<Self>,
    ) {
        self.mini_apps.confirm = Some(confirm);
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

    fn spawn_mini_app_window(&mut self, launch: WebAppLaunch, cx: &mut Context<Self>) {
        let Some(path) = helper_path() else {
            self.set_status_note("The mini app window (quill-webview) is missing.", cx);
            return;
        };
        let account = self
            .live
            .as_ref()
            .map(|live| live.driver.session.account.to_string())
            .unwrap_or_else(|| "default".into());
        let mut command = Command::new(path);
        command
            .arg("--title")
            .arg(&launch.bot_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The helper is quiet unless `QUILL_WEBVIEW_DEBUG` is set, in
            // which case its trace goes to Quill's stderr.
            .stderr(if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        if let Some(dir) = data_dir(&account, launch.bot_user_id) {
            let _ = std::fs::create_dir_all(&dir);
            command.arg("--data-dir").arg(dir);
        }
        command
            .arg("--data-id")
            .arg(data_store_id(&account, launch.bot_user_id));
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                self.set_status_note("The mini app window couldn't start.", cx);
                return;
            }
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return;
        };
        let (sender, events) = mpsc::channel();
        std::thread::Builder::new()
            .name("quill-webview-reader".into())
            .spawn(move || {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.len() > quill_webview_protocol::MAX_IPC_MESSAGE_BYTES * 2 {
                        continue;
                    }
                    if let Ok(event) = decode::<HelperEvent>(&line)
                        && sender.send(event).is_err()
                    {
                        break;
                    }
                }
                let _ = sender.send(HelperEvent::Closed);
            })
            .ok();
        let in_attachment_menu = self.session().is_some_and(|session| {
            session
                .web_apps
                .attachment_menu_bots
                .iter()
                .any(|bot| bot.bot_user_id == launch.bot_user_id && bot.is_added)
        });
        let privacy_policy_url = self
            .session()
            .and_then(|session| session.bot_info.get(&launch.bot_user_id))
            .and_then(|info| info.as_ref())
            .map(|info| info.privacy_policy_url.clone())
            .filter(|url| quill::text::openable_http_url(url));
        self.mini_apps.window = Some(MiniAppWindow {
            child,
            stdin,
            events,
            launch,
            theme: theme_params(cx),
            ready: false,
            closing_confirmation: false,
            settings_button: false,
            popups: HashMap::new(),
            popup_seq: 0,
            waiting_write_access: false,
            data_sent: false,
            privacy_policy_url,
            in_attachment_menu,
        });
        self.ensure_mini_app_poll(cx);
    }

    /// Close the window (and tell TDLib) without waiting for the helper.
    pub(super) fn close_mini_app_window(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.mini_apps.window.take() else {
            return;
        };
        let launch_id = window.launch.launch_id;
        drop(window);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.close_web_app(launch_id);
        }
        cx.notify();
    }

    /// Poll the helper and the session's mini-app answers while something
    /// is going on; stops by itself when nothing is left.
    fn ensure_mini_app_poll(&mut self, cx: &mut Context<Self>) {
        if self.mini_apps.poll.is_some() {
            return;
        }
        self.mini_apps.generation += 1;
        let generation = self.mini_apps.generation;
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        this.mini_apps.generation == generation && this.poll_mini_apps(cx)
                    })
                    .unwrap_or(false);
                if !keep {
                    let _ = this.update(cx, |this, _| {
                        if this.mini_apps.generation == generation {
                            this.mini_apps.poll = None;
                        }
                    });
                    break;
                }
            }
        });
        self.mini_apps.poll = Some(task);
    }

    /// One poll step; `true` while there is still something to wait for.
    fn poll_mini_apps(&mut self, cx: &mut Context<Self>) -> bool {
        self.poll_mini_app_session(cx);
        let events: Vec<HelperEvent> = match self.mini_apps.window.as_ref() {
            Some(window) => window.events.try_iter().collect(),
            None => Vec::new(),
        };
        for event in events {
            self.handle_helper_event(event, cx);
        }
        self.mini_apps.window.is_some()
            || self.mini_apps.awaiting_attach_bot.is_some()
            || self.mini_apps.awaiting_search.is_some()
            || self.mini_apps.awaiting_toggle.is_some()
            || self
                .session()
                .is_some_and(|session| session.web_apps.pending.is_some())
    }

    /// The session's one-shot answers.
    fn poll_mini_app_session(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let apps = &mut live.driver.session.web_apps;
        let open_result = apps.open_result.take();
        let write_access = apps.write_access.take();
        let custom_replies = std::mem::take(&mut apps.custom_replies);
        let attach_bot = apps.attachment_menu_bot.take();
        let toggled = apps.attachment_menu_toggled.take();
        let found = apps.found.take();
        let data_sent = apps.data_sent.take();

        if let Some(result) = open_result {
            match result {
                WebAppOpenResult::Opened {
                    bot_user_id,
                    chat_id,
                    source,
                    launch_id,
                    url,
                } => {
                    let bot_name = self
                        .session()
                        .and_then(|session| session.users.get(&bot_user_id))
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| "Mini app".into());
                    self.status_note.clear();
                    self.spawn_mini_app_window(
                        WebAppLaunch {
                            bot_user_id,
                            bot_name,
                            chat_id,
                            source,
                            launch_id,
                            url,
                        },
                        cx,
                    );
                }
                WebAppOpenResult::Failed { message, .. } => self.set_status_note(&message, cx),
            }
        }
        if let Some((bot_id, result)) = write_access {
            self.finish_write_access(bot_id, result, cx);
        }
        for reply in custom_replies {
            if let Some(window) = self.mini_apps.window.as_mut() {
                let reply = match reply.result {
                    Ok(json) => BridgeReply::custom_method_result(
                        &reply.req_id,
                        serde_json::from_str(&json).unwrap_or(serde_json::Value::Null),
                    ),
                    Err(error) => BridgeReply::custom_method_error(&reply.req_id, &error),
                };
                window.emit(reply);
            }
        }
        if let Some(result) = attach_bot
            && let Some(then) = self.mini_apps.awaiting_attach_bot.take()
        {
            match result {
                Ok(bot) if bot.is_added => self.start_web_app(then, cx),
                Ok(bot) => {
                    self.mini_apps.confirm = Some(WebAppConfirm::AddToMenu {
                        bot,
                        allow_write: true,
                        then,
                    });
                    cx.notify();
                }
                Err(message) => self.set_status_note(&message, cx),
            }
        }
        if let Some(result) = toggled {
            match result {
                Ok((_, added)) => {
                    self.set_status_note(
                        if added {
                            "Bot added to the menu."
                        } else {
                            "Bot removed from the menu."
                        },
                        cx,
                    );
                    if let Some(then) = self.mini_apps.awaiting_toggle.take() {
                        self.request_web_app(then, false, cx);
                    }
                }
                Err(message) => {
                    self.mini_apps.awaiting_toggle = None;
                    self.set_status_note(&message, cx);
                }
            }
        }
        if let Some(result) = found
            && let Some(then) = self.mini_apps.awaiting_search.take()
        {
            match result {
                Ok((_, app)) => {
                    let bot_id = then.bot_id();
                    let verified = self
                        .session()
                        .and_then(|session| session.users.get(&bot_id))
                        .is_some_and(|user| user.verification.is_verified);
                    if app.skip_confirmation
                        || (!app.request_write_access
                            && !trust::needs_confirmation(bot_id, verified, false))
                    {
                        self.request_web_app(then, false, cx);
                    } else {
                        self.mini_apps.confirm = Some(WebAppConfirm::OpenTerms {
                            then,
                            request_write: app.request_write_access,
                            allow_write: true,
                        });
                        cx.notify();
                    }
                }
                Err(message) => self.set_status_note(&message, cx),
            }
        }
        if let Some(result) = data_sent {
            match result {
                Ok(()) => self.set_status_note("Sent to the bot.", cx),
                Err(message) => self.set_status_note(&message, cx),
            }
        }
    }

    fn finish_write_access(
        &mut self,
        bot_id: i64,
        result: WriteAccessResult,
        cx: &mut Context<Self>,
    ) {
        let Some(window) = self.mini_apps.window.as_mut() else {
            return;
        };
        if window.launch.bot_user_id != bot_id || !window.waiting_write_access {
            return;
        }
        match result {
            WriteAccessResult::Allowed | WriteAccessResult::Granted => {
                window.waiting_write_access = false;
                window.emit(BridgeReply::write_access_requested(true));
            }
            WriteAccessResult::Denied => {
                window.waiting_write_access = false;
                window.emit(BridgeReply::write_access_requested(false));
            }
            WriteAccessResult::NeedsConsent => {
                let name = window.launch.bot_name.clone();
                window.show_popup(
                    PopupPurpose::WriteAccess,
                    popup(
                        "Allow messaging",
                        format!("Allow {name} to send you messages?"),
                        vec![
                            ("cancel", "Cancel", "cancel"),
                            ("allow", "Allow", "default"),
                        ],
                    ),
                );
            }
        }
        cx.notify();
    }

    fn handle_helper_event(&mut self, event: HelperEvent, cx: &mut Context<Self>) {
        match event {
            HelperEvent::Ready => {
                let Some(window) = self.mini_apps.window.as_mut() else {
                    return;
                };
                window.ready = true;
                let menu = window.menu();
                let command = HostCommand::Load {
                    url: window.launch.url.clone(),
                    title: window.launch.bot_name.clone(),
                    theme: window.theme.bridge_colors(),
                    menu,
                };
                window.send(&command);
            }
            HelperEvent::WebApp { event, data } => {
                // Unknown or malformed events are dropped, as tdesktop does.
                if let Ok(parsed) = parse_event(&event, &data) {
                    self.handle_web_app_event(parsed, cx);
                }
            }
            HelperEvent::Shell { event } => self.handle_shell_event(event, cx),
            HelperEvent::OpenExternal { url } => self.open_from_mini_app(&url, cx),
            HelperEvent::Closed => self.close_mini_app_window(cx),
            HelperEvent::Error { message } => {
                let note = if message.contains("web view") {
                    if cfg!(target_os = "windows") {
                        "Mini apps need the Microsoft Edge WebView2 runtime.".to_string()
                    } else if cfg!(target_os = "linux") {
                        "Mini apps need WebKitGTK (libwebkit2gtk-4.1) installed.".to_string()
                    } else {
                        "The mini app window couldn't create its web view.".to_string()
                    }
                } else {
                    format!("The mini app window failed: {message}")
                };
                self.close_mini_app_window(cx);
                self.set_status_note(&note, cx);
            }
        }
    }

    /// A URL the page tried to leave the frame for.
    fn open_from_mini_app(&mut self, url: &str, cx: &mut Context<Self>) {
        if is_telegram_link(url) {
            self.pending_deep_link = Some(url.to_string());
            cx.notify();
            return;
        }
        if !quill::text::openable_http_url(url) {
            return;
        }
        let link = LinkTarget::Url {
            url: url.to_string(),
            label: None,
        };
        match open_decision(&link) {
            OpenDecision::Open => self.open_message_url(url, cx),
            OpenDecision::Confirm {
                url,
                shown,
                suspicious,
            } => {
                self.message_ui.open_link_confirm = Some(super::entity_links::OpenLinkConfirm {
                    url,
                    shown,
                    suspicious,
                });
                cx.activate(true);
                cx.notify();
            }
        }
    }

    fn with_window(&mut self, f: impl FnOnce(&mut MiniAppWindow)) {
        if let Some(window) = self.mini_apps.window.as_mut() {
            f(window);
        }
    }

    fn handle_web_app_event(&mut self, event: WebAppEvent, cx: &mut Context<Self>) {
        let Some((bot_id, bot_name, source, data_sent, waiting_write_access)) =
            self.mini_apps.window.as_ref().map(|window| {
                (
                    window.launch.bot_user_id,
                    window.launch.bot_name.clone(),
                    window.launch.source.clone(),
                    window.data_sent,
                    window.waiting_write_access,
                )
            })
        else {
            return;
        };
        match event {
            WebAppEvent::Ready | WebAppEvent::RequestTheme | WebAppEvent::RequestViewport => {}
            WebAppEvent::Haptic => {}
            WebAppEvent::Close => self.close_mini_app_window(cx),
            WebAppEvent::DataSend { data } => {
                if !source.allows_data_send() || data_sent {
                    return;
                }
                self.with_window(|window| window.data_sent = true);
                let button_text = match &source {
                    LaunchSource::KeyboardButton { button_text } => button_text.clone(),
                    _ => String::new(),
                };
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.send_web_app_data(bot_id, &button_text, &data));
                if !matches!(sent, Some(Ok(_))) {
                    self.set_status_note("Couldn't send the app's data to the bot.", cx);
                }
                // tdesktop closes the panel once the data is on its way.
                self.close_mini_app_window(cx);
            }
            WebAppEvent::SwitchInlineQuery { query, chat_types } => {
                if chat_types.is_empty() {
                    let username = self
                        .session()
                        .and_then(|session| session.users.get(&bot_id))
                        .map(|user| user.username.clone())
                        .unwrap_or_default();
                    if !username.is_empty() {
                        self.mini_apps.render_action = Some(RenderAction::SwitchInline {
                            query: format!("@{username} {query}"),
                        });
                        self.close_mini_app_window(cx);
                        cx.activate(true);
                    }
                }
            }
            WebAppEvent::SetupMainButton(setup) => self.with_window(|window| {
                window.send(&HostCommand::MainButton {
                    button: bottom_button(setup),
                });
            }),
            WebAppEvent::SetupSecondaryButton(setup) => self.with_window(|window| {
                window.send(&HostCommand::SecondaryButton {
                    button: bottom_button(setup),
                });
            }),
            WebAppEvent::SetupBackButton { visible } => self.with_window(|window| {
                window.send(&HostCommand::BackButton { visible });
            }),
            WebAppEvent::SetupSettingsButton { visible } => self.with_window(|window| {
                window.settings_button = visible;
                let menu = window.menu();
                window.send(&HostCommand::SetMenu { menu });
            }),
            WebAppEvent::OpenLink { url, .. } => self.open_from_mini_app(&url, cx),
            WebAppEvent::OpenTgLink { url } => {
                self.pending_deep_link = Some(url);
                cx.activate(true);
                cx.notify();
            }
            WebAppEvent::OpenInvoice { slug } => self.with_window(|window| {
                window.emit(BridgeReply::invoice_closed(&slug, "cancelled"));
                window.send(&HostCommand::Toast {
                    text: "Payments inside mini apps aren't supported yet.".into(),
                });
            }),
            WebAppEvent::OpenPopup(spec) => self.with_window(|window| {
                window.show_popup(
                    PopupPurpose::App,
                    Popup {
                        id: String::new(),
                        title: spec.title,
                        message: spec.message,
                        buttons: spec
                            .buttons
                            .into_iter()
                            .map(|button| PopupButton {
                                id: button.id,
                                text: button.text,
                                kind: button.kind,
                            })
                            .collect(),
                    },
                );
            }),
            WebAppEvent::RequestWriteAccess => {
                if waiting_write_access {
                    self.with_window(|window| {
                        window.emit(BridgeReply::write_access_requested(false));
                    });
                    return;
                }
                self.with_window(|window| window.waiting_write_access = true);
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.can_bot_send_messages(bot_id));
                if !matches!(sent, Some(Ok(_))) {
                    self.with_window(|window| {
                        window.waiting_write_access = false;
                        window.emit(BridgeReply::write_access_requested(false));
                    });
                }
            }
            WebAppEvent::RequestPhone => self.with_window(|window| {
                window.emit(BridgeReply::phone_requested(false));
                window.send(&HostCommand::Toast {
                    text: "Sharing your phone number with mini apps isn't supported yet.".into(),
                });
            }),
            WebAppEvent::ReadTextFromClipboard { req_id } => self.with_window(|window| {
                window.show_popup(
                    PopupPurpose::Clipboard { req_id },
                    popup(
                        "Paste from the clipboard?",
                        format!("{bot_name} asks to read what you copied."),
                        vec![
                            ("cancel", "Cancel", "cancel"),
                            ("allow", "Paste", "default"),
                        ],
                    ),
                );
            }),
            WebAppEvent::SetupClosingBehavior { need_confirmation } => {
                self.with_window(|window| window.closing_confirmation = need_confirmation);
            }
            WebAppEvent::SetHeaderColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::HeaderColor { color });
            }),
            WebAppEvent::SetBackgroundColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::BackgroundColor { color });
            }),
            WebAppEvent::SetBottomBarColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::BottomBarColor { color });
            }),
            WebAppEvent::InvokeCustomMethod {
                req_id,
                method,
                params,
            } => {
                let sent = self.live.as_mut().map(|live| {
                    live.driver.send_web_app_custom_request(
                        bot_id,
                        &req_id,
                        &method,
                        &params.to_string(),
                    )
                });
                if !matches!(sent, Some(Ok(_))) {
                    self.with_window(|window| {
                        window.emit(BridgeReply::custom_method_error(&req_id, "UNKNOWN_ERROR"));
                    });
                }
            }
            WebAppEvent::Unsupported { reply, .. } => {
                if let Some(reply) = reply {
                    self.with_window(|window| window.emit(reply));
                }
            }
        }
    }

    fn handle_shell_event(&mut self, event: ShellEvent, cx: &mut Context<Self>) {
        let Some((bot_id, closing_confirmation, privacy_policy_url)) =
            self.mini_apps.window.as_ref().map(|window| {
                (
                    window.launch.bot_user_id,
                    window.closing_confirmation,
                    window.privacy_policy_url.clone(),
                )
            })
        else {
            return;
        };
        match event {
            ShellEvent::Close { force } => {
                if closing_confirmation && !force {
                    self.with_window(|window| {
                        window.show_popup(
                            PopupPurpose::CloseConfirm,
                            popup(
                                "Close the mini app?",
                                "Changes you made may not be saved.".into(),
                                vec![
                                    ("cancel", "Cancel", "cancel"),
                                    ("close", "Close anyway", "destructive"),
                                ],
                            ),
                        );
                    });
                } else {
                    self.close_mini_app_window(cx);
                }
            }
            ShellEvent::Back | ShellEvent::Reload | ShellEvent::FrameLoaded => {}
            ShellEvent::Menu { id } => match id.as_str() {
                "open_bot" => {
                    self.mini_apps.render_action = Some(RenderAction::OpenBotChat { bot_id });
                    cx.activate(true);
                    cx.notify();
                }
                "terms" => self.open_message_url(MINI_APP_TERMS_URL, cx),
                "privacy" => {
                    if let Some(url) = privacy_policy_url {
                        self.open_message_url(&url, cx);
                    }
                }
                "remove_from_menu" => {
                    let sent = self.live.as_mut().map(|live| {
                        live.driver
                            .toggle_bot_in_attachment_menu(bot_id, false, false)
                    });
                    if matches!(sent, Some(Ok(_))) {
                        self.close_mini_app_window(cx);
                    }
                }
                _ => {}
            },
            ShellEvent::PopupClosed { id, button } => {
                let Some(purpose) = self
                    .mini_apps
                    .window
                    .as_mut()
                    .and_then(|window| window.popups.remove(&id))
                else {
                    return;
                };
                match purpose {
                    PopupPurpose::App => self.with_window(|window| {
                        window.emit(BridgeReply::popup_closed(&button));
                    }),
                    PopupPurpose::WriteAccess => {
                        let sent = (button == "allow")
                            .then(|| {
                                self.live
                                    .as_mut()
                                    .map(|live| live.driver.allow_bot_to_send_messages(bot_id))
                            })
                            .flatten();
                        if !matches!(sent, Some(Ok(_))) {
                            self.with_window(|window| {
                                window.waiting_write_access = false;
                                window.emit(BridgeReply::write_access_requested(false));
                            });
                        }
                    }
                    PopupPurpose::Clipboard { req_id } => {
                        let text = (button == "allow")
                            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
                            .flatten();
                        self.with_window(|window| {
                            window.emit(BridgeReply::clipboard_text_received(
                                &req_id,
                                text.as_deref(),
                            ));
                        });
                    }
                    PopupPurpose::CloseConfirm => {
                        if button == "close" {
                            self.close_mini_app_window(cx);
                        }
                    }
                }
            }
        }
    }

    /// Render-time half: what needs the window.
    pub(super) fn run_pending_mini_app_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.mini_apps.render_action.take() else {
            return;
        };
        match action {
            RenderAction::SwitchInline { query } => {
                self.insert_switch_inline_query(&query, window, cx);
            }
            RenderAction::OpenBotChat { bot_id } => self.open_user_chat(bot_id, window, cx),
        }
    }

    /// Whether the open window shows `bot_id`'s app (the Apps tab row).
    pub(super) fn mini_app_open_for(&self, bot_id: i64) -> bool {
        self.mini_apps
            .window
            .as_ref()
            .is_some_and(|window| window.launch.bot_user_id == bot_id)
    }
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

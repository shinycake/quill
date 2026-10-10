//! Clicking, right-clicking and hovering the interactive entities of a
//! message: @mentions, #hashtags, bot commands, links and the rest.
//!
//! Telegram Desktop gives each entity a click handler (`core/click_handler_types.cpp`):
//! a mention resolves the username, a hashtag searches, a command is sent,
//! a hidden link asks first (`quill::link_policy`), phone numbers and
//! cards offer a small menu. A right-click adds the link's copy entry to
//! the message menu, and a text link shows its address as a tooltip.
//!
//! The text element only knows a click happened; [`QuillApp::queue_link`]
//! parks it and [`QuillApp::run_pending_link`] (called from render, which
//! owns the `Window`) acts on it.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ComposerSnapshot;
use quill::ids::{ChatId, MessageId};
use quill::link_policy::{OpenDecision, open_decision};
use quill::telegram::envelope::{ChatKind, MessageContent, MessageSender};
use quill::text::LinkTarget;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

/// How long the pointer rests on a link before its tooltip shows.
const TOOLTIP_DELAY: Duration = Duration::from_millis(450);

/// A clicked link waiting for render: the link and the message it is in
/// (chat id, message id).
pub(super) struct PendingLink {
    link: LinkTarget,
    msg_key: (i64, u64),
}

/// The "Open this link?" box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OpenLinkConfirm {
    pub(super) url: String,
    pub(super) shown: String,
    pub(super) suspicious: Vec<Range<usize>>,
}

/// The small menu a phone number, card number or date opens on click.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LinkPopup {
    pub(super) position: Point<Pixels>,
    pub(super) link: LinkTarget,
}

/// The tooltip of the link under the pointer.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LinkTooltip {
    position: Point<Pixels>,
    text: SharedString,
    token: u64,
    shown: bool,
}

impl QuillApp {
    /// A link was clicked: act on it at the next render.
    pub(super) fn queue_link(
        &mut self,
        link: LinkTarget,
        msg_key: (i64, u64),
        cx: &mut Context<Self>,
    ) {
        self.message_ui.link_tooltip = None;
        self.message_ui.pending_link = Some(PendingLink { link, msg_key });
        cx.notify();
    }

    /// Render-time half of [`Self::queue_link`].
    pub(super) fn run_pending_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(PendingLink { link, msg_key }) = self.message_ui.pending_link.take() else {
            return;
        };
        let chat_id = ChatId(msg_key.0);
        let message_id = MessageId(msg_key.1 as i64);
        match link {
            LinkTarget::Url { .. } => match open_decision(&link) {
                OpenDecision::Open => {
                    if let LinkTarget::Url { url, .. } = &link {
                        self.open_message_url(url, cx);
                    }
                }
                OpenDecision::Confirm {
                    url,
                    shown,
                    suspicious,
                } => {
                    self.message_ui.open_link_confirm = Some(OpenLinkConfirm {
                        url,
                        shown,
                        suspicious,
                    });
                }
            },
            LinkTarget::Mention(name) => {
                let username = name.trim_start_matches('@');
                if self.live.is_some() {
                    self.pending_deep_link = Some(format!("https://t.me/{username}"));
                    self.connection.status_note = format!("opening @{username}…");
                } else {
                    self.connection.status_note =
                        "can't open @mentions without a Telegram connection".into();
                }
            }
            LinkTarget::MentionName { user_id, .. } => {
                self.open_avatar_profile(MessageSender::User { user_id }, window, cx);
            }
            LinkTarget::Hashtag(tag) | LinkTarget::Cashtag(tag) => {
                self.search_for_tag(chat_id, &tag, window, cx);
            }
            LinkTarget::BotCommand(command) => {
                self.send_bot_command(chat_id, message_id, &command, cx);
            }
            LinkTarget::Email(address) => {
                self.connection.status_note = if quill::platform::open_mailto(&address) {
                    "opened mail".into()
                } else {
                    "could not open the mail app".into()
                };
            }
            LinkTarget::Phone(_) | LinkTarget::BankCard(_) | LinkTarget::DateTime { .. } => {
                self.message_ui.link_popup = Some(LinkPopup {
                    position: window.mouse_position(),
                    link,
                });
            }
            LinkTarget::MediaTimestamp { seconds } => {
                self.seek_media_timestamp(chat_id, message_id, seconds, cx);
            }
        }
        cx.notify();
    }

    /// Copy `text` and say so (Telegram Desktop's "Text copied" toast).
    pub(super) fn copy_entity_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.connection.status_note = "text copied to clipboard".into();
        cx.notify();
    }

    /// Hashtags and cashtags search, as Telegram Desktop: inside a group or
    /// channel, in that chat; from a private chat, everywhere.
    fn search_for_tag(
        &mut self,
        chat_id: ChatId,
        tag: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_chat = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    ChatKind::Supergroup { .. } | ChatKind::BasicGroup { .. }
                )
            })
            && self.session().is_some_and(|s| s.open_chat == Some(chat_id));
        if in_chat {
            self.open_chat_search_ui(window, cx);
            let query = tag.to_string();
            self.search_ui
                .chat_input
                .update(cx, |input, cx| input.set_value(&query, window, cx));
            self.sync_chat_search_query(tag, cx);
        } else {
            self.open_search_ui(window, cx);
            let query = tag.to_string();
            self.search_ui
                .input
                .update(cx, |input, cx| input.set_value(&query, window, cx));
            self.sync_search_query(tag, cx);
        }
    }

    /// A clicked `/command`: send it to the chat, adding `@bot` in a group
    /// when the message came from a bot (`Bot::WrapCommandInChat`).
    fn send_bot_command(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        let session = self.live.as_ref().map(|live| &live.driver.session);
        let session = session.or(self.demo_session.as_ref());
        let Some(session) = session else {
            return;
        };
        if session.open_chat != Some(chat_id) {
            return;
        }
        let in_group = session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup { .. } | ChatKind::BasicGroup { .. }
            )
        });
        let bot_username = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.sender)
            .and_then(|sender| match sender {
                MessageSender::User { user_id } => session.users.get(&user_id),
                MessageSender::Chat { .. } => None,
            })
            .filter(|user| user.is_bot && !user.username.is_empty())
            .map(|user| user.username.clone());
        let text = match bot_username {
            Some(bot) if in_group && !command.contains('@') => format!("{command}@{bot}"),
            _ => command.to_string(),
        };
        let view_generation = session.view_generation;
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let snapshot = ComposerSnapshot::capture(chat_id, view_generation, &text);
            self.connection.status_note = match live.driver.send_snapshot(&snapshot) {
                Ok(_) => "sending…".into(),
                Err(_) => "could not send the command".into(),
            };
        } else {
            self.apply_demo_outgoing(&text, None, None);
            self.connection.status_note = "demo send applied locally (no live Telegram)".into();
        }
    }

    /// A media-timestamp link: seek the message's own voice, audio or
    /// video, or the one it replies to; a YouTube preview opens at the
    /// moment (Telegram Desktop `OpenMediaTimestamp`).
    pub(super) fn seek_media_timestamp(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        seconds: i32,
        cx: &mut Context<Self>,
    ) {
        use quill::media_timestamp::{SeekTarget, clamp_seek, seek_target};
        let Some(session) = self.session() else {
            return;
        };
        let history = session.histories.get(&chat_id.0);
        let message = |id: i64| history.and_then(|history| history.messages.get(&id));
        let own = message(message_id.0);
        let replied = own
            .and_then(|message| message.reply_to.as_ref())
            .filter(|reply| reply.is_same_chat(chat_id))
            .and_then(|reply| message(reply.message_id.0));
        let target = [own, replied]
            .into_iter()
            .flatten()
            .find_map(|message| Some((message, seek_target(&message.content, seconds)?)));
        let Some((target, kind)) = target else {
            self.connection.status_note = "nothing to seek".into();
            return;
        };
        let target_id = target.id;
        match (&target.content, kind) {
            (MessageContent::VoiceNote(voice), SeekTarget::VoiceNote) => {
                let (file_id, listened, duration) =
                    (voice.file_id, voice.is_listened, voice.duration);
                let secs = clamp_seek(seconds, duration);
                self.playback.positions.insert(target_id, secs);
                if self.playback.playing_voice == Some(target_id)
                    && self.playback.player.chat.is_none_or(|c| c == chat_id)
                {
                    self.seek_active_to(secs, cx);
                } else {
                    self.toggle_voice_playback(
                        chat_id,
                        target_id,
                        file_id,
                        listened,
                        f64::from(duration),
                        cx,
                    );
                }
            }
            (MessageContent::Audio(audio), SeekTarget::Audio) => {
                let (file_id, duration) = (audio.file_id, audio.duration);
                let secs = clamp_seek(seconds, duration);
                self.playback.positions.insert(target_id, secs);
                if self.playback.playing_audio == Some(target_id)
                    && self.playback.player.chat.is_none_or(|c| c == chat_id)
                {
                    self.seek_active_to(secs, cx);
                } else {
                    self.toggle_audio_playback(
                        chat_id,
                        target_id,
                        file_id,
                        f64::from(duration),
                        cx,
                    );
                }
            }
            (MessageContent::Video(video), SeekTarget::Video) => {
                self.viewer.pending_seek = Some((target_id, clamp_seek(seconds, video.duration)));
                self.open_media_viewer(chat_id, target_id, cx);
            }
            (_, SeekTarget::Web(url)) => self.open_message_url(&url, cx),
            _ => {}
        }
    }

    /// Remember a right-press on a link; the message menu picks it up if
    /// it opens at this very point.
    pub(super) fn note_right_clicked_link(&mut self, position: Point<Pixels>, link: LinkTarget) {
        self.message_ui.link_tooltip = None;
        self.message_ui.right_clicked_link = Some((position, link));
    }

    /// The link under the right-click that opened a menu at `position`.
    pub(super) fn take_right_clicked_link(
        &mut self,
        position: Point<Pixels>,
    ) -> Option<LinkTarget> {
        self.message_ui
            .right_clicked_link
            .take()
            .filter(|(at, _)| *at == position)
            .map(|(_, link)| link)
    }

    /// The pointer entered a link that has a tooltip (or left one).
    pub(super) fn set_link_tooltip(
        &mut self,
        hover: Option<(Point<Pixels>, String)>,
        cx: &mut Context<Self>,
    ) {
        match hover {
            Some((position, text)) => {
                let token = self.message_ui.link_tooltip.as_ref().map_or(0, |t| t.token) + 1;
                self.message_ui.link_tooltip = Some(LinkTooltip {
                    position,
                    text: text.into(),
                    token,
                    shown: false,
                });
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(TOOLTIP_DELAY).await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(tooltip) = this.message_ui.link_tooltip.as_mut()
                            && tooltip.token == token
                        {
                            tooltip.shown = true;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            None => {
                if self.message_ui.link_tooltip.take().is_some_and(|t| t.shown) {
                    cx.notify();
                }
            }
        }
    }

    /// The link tooltip, once the pointer has rested on the link.
    pub(super) fn link_tooltip_overlay(&self) -> Option<AnyElement> {
        let tooltip = self.message_ui.link_tooltip.as_ref().filter(|t| t.shown)?;
        Some(
            div()
                .id("link-tooltip")
                .absolute()
                .left(tooltip.position.x + px(12.))
                .top(tooltip.position.y + px(18.))
                .max_w(px(420.))
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(accent())
                .bg(bg_canvas())
                .text_xs()
                .text_color(text_menu())
                .child(tooltip.text.clone())
                .into_any_element(),
        )
    }

    /// The menu of a phone number, card number or date.
    pub(super) fn link_popup_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.message_ui.link_popup.clone()?;
        let (label, text) = match &popup.link {
            LinkTarget::Phone(number) => ("Copy Phone Number", number.clone()),
            LinkTarget::BankCard(number) => ("Copy Card Number", number.clone()),
            LinkTarget::DateTime { unix_time, label } => (
                "Copy Date",
                if label.trim().is_empty() {
                    super::message_text::format_unix_date_time(i64::from(*unix_time))
                } else {
                    label.clone()
                },
            ),
            _ => return None,
        };
        let row_hover = cx.theme().accent;
        Some(
            div()
                .id("link-popup-overlay")
                .occlude()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("link-popup-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.message_ui.link_popup = None;
                            cx.notify();
                        })),
                )
                .child(
                    anchored()
                        .position(popup.position)
                        .snap_to_window_with_margin(px(8.))
                        .child(
                            div()
                                .id("link-popup-panel")
                                .occlude()
                                .min_w(px(180.))
                                .px_1()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(accent())
                                .bg(bg_canvas())
                                .child(
                                    div()
                                        .id("link-popup-copy")
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .px_3()
                                        .py_1p5()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .text_sm()
                                        .text_color(text_menu())
                                        .hover(|style| style.bg(row_hover))
                                        .role(gpui_kit::Role::MenuItem)
                                        .aria_label(label)
                                        .child(
                                            Icon::new(gpui_kit::assets::IconName::Copy)
                                                .size(px(16.)),
                                        )
                                        .child(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.message_ui.link_popup = None;
                                            this.copy_entity_text(text.clone(), cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// kit `Dialog` for the "Open this link?" box (`DialogKind::OpenLink`).
    pub(super) fn build_open_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::OpenLink, |this, _, cx| {
                this.message_ui.open_link_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let confirm = this
                .message_ui
                .open_link_confirm
                .clone()
                .unwrap_or(OpenLinkConfirm {
                    url: String::new(),
                    shown: String::new(),
                    suspicious: Vec::new(),
                });
            let marked = !confirm.suspicious.is_empty();
            let mut highlights: Vec<(Range<usize>, HighlightStyle)> = confirm
                .suspicious
                .iter()
                .map(|range| {
                    (
                        range.clone(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            color: Some(danger_bright().into()),
                            ..Default::default()
                        },
                    )
                })
                .collect();
            if !marked
                && let Some(host) = quill::link_policy::host_of(&confirm.shown)
                && let Some(at) = confirm.shown.to_lowercase().find(&host)
            {
                // Telegram Desktop bolds the domain of an ordinary address.
                highlights.push((
                    at..at + host.len(),
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ));
            }
            let url = confirm.url.clone();
            let shown = confirm.shown.clone();
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(StyledText::new(confirm.shown.clone()).with_highlights(highlights)),
                )
                .when(marked, |this| {
                    this.child(div().text_sm().text_color(text_muted()).child(
                        "Highlighted characters are from a different alphabet than the \
                         rest of the address.",
                    ))
                })
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("open-link-copy")
                        .label("Copy Link")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_entity_text(shown.clone(), cx);
                        })),
                )
                .child(
                    Button::new("open-link-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.message_ui.open_link_confirm = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::OpenLink, window, cx);
                        })),
                )
                .child(
                    Button::new("open-link-open")
                        .label("Open")
                        .primary()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.message_ui.open_link_confirm = None;
                            this.open_message_url(&url, cx);
                            this.close_kit_dialog_if_done(DialogKind::OpenLink, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Open this link?"))
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
}

crate::ui::shell::register_dialogs! {
    /// "Open this link?" for a hidden or look-alike message link.
    OpenLink => DialogSpec::new(
        3300,
        |app| app.message_ui.open_link_confirm.is_some(),
        QuillApp::build_open_link_dialog,
    ),
}

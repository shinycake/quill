//! Methods moved out of `composer.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

/// See [`QuillApp::send_options_state`].
#[derive(Clone, Copy)]
pub(in crate::ui) struct SendOptionsState {
    silent: bool,
    preview_off: bool,
    scheduled: bool,
    kind: quill::schedule::ScheduleKind,
}

impl QuillApp {
    /// M1: the composer's `messageSendOptions` for the next send.
    pub(in crate::ui) fn composer_send_options(&self) -> SendOptions {
        let send = self.pending_send();
        SendOptions {
            disable_notification: self.composer_effective_silent()
                || self.composer_ui.send_silent_once,
            scheduling: send.scheduling,
            schedule_repeat_period: match send.scheduling {
                ComposerScheduling::SendAtDate(_) => send.schedule_repeat,
                _ => 0,
            },
            link_preview_disabled: send.preview.disabled,
            link_preview_above_text: send.preview.above_text,
            link_preview_media: send.preview.media,
            link_preview_link: send.preview.link_index,
            // The driver overrides this for secret chats at send time.
            is_secret: false,
            effect_id: self.composer_ui.send_effect_once,
            ..SendOptions::default()
        }
    }

    /// M1: apply a formatting action to the composer selection. The field
    /// shows formatting (codex:composer-input), so the selection gets the
    /// format, or loses it when it has it already; with nothing selected the
    /// next typed text does. The rich editor holds block markup, so there the
    /// action still writes markers.
    pub(in crate::ui) fn apply_composer_format(
        &mut self,
        action: FormatAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer_ui.rich_editor_open {
            let tag = super::composer_field::tag_for_action(&action);
            self.toggle_composer_tag(tag, window, cx);
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let (new_text, new_selection) = apply_format_markup(&text, range, &action);
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
            input.set_selected_range(new_selection, cx);
        });
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// M1: strip formatting markers in the composer selection (whole text
    /// when the selection is empty), keeping the inner text.
    pub(in crate::ui) fn clear_composer_format(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer_ui.rich_editor_open {
            self.clear_composer_tags(window, cx);
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let new_text = clear_format_markup(&text, range);
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
        });
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// M1: load the chat's scheduled sends and open the dialog.
    pub(in crate::ui) fn open_scheduled_dialog(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.schedule_popup_open = false;
        if let Some(live) = self.live.as_mut()
            && let Some(chat_id) = live.driver.session.open_chat
        {
            match live.driver.get_chat_scheduled_messages(chat_id) {
                Ok(_) => self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_CHAT_LOADING_SCHEDULED_MESSAGES,
                ))),
                Err(_) => self.connection.set_note(StatusNote::error(tr(
                    keys::Q_CHAT_COULD_NOT_LOAD_SCHEDULED_MESSAGES,
                ))),
            }
        }
        self.composer_ui.scheduled_dialog_open = true;
        cx.notify();
    }

    /// M1: delete a scheduled send (`deleteMessages`, revoke false —
    /// scheduled messages are not in history, so this goes through the
    /// driver's scheduled-message path, not the confirm dialog).
    pub(in crate::ui) fn delete_scheduled_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(chat_id) = live.driver.session.open_chat else {
            return;
        };
        match live.driver.delete_scheduled_message(chat_id, message_id) {
            Ok(_) => {
                live.driver
                    .session
                    .messages
                    .scheduled_messages
                    .retain(|m| m.id != message_id);
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_CHAT_SCHEDULED_MESSAGE_DELETED,
                )));
            }
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_CHAT_COULD_NOT_DELETE_SCHEDULED_MESSAGE,
            ))),
        }
        cx.notify();
    }

    /// M1: the composer's formatting menu (Telegram X `InputView` format
    /// menu / tdesktop markdown behavior): bold, italic, underline,
    /// strikethrough, inline code, code block, spoiler, quote, link, and
    /// clear-formatting. Formatting applies to the textarea selection via
    /// `apply_format_markup`; the send path converts markup to TDLib
    /// `textEntities` (`parse_format_markup`).
    pub(in crate::ui) fn format_menu_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        Button::new("composer-format-menu")
            .icon(IconName::ALargeSmall)
            .ghost()
            .tooltip(tr(keys::Q_CHAT_FORMATTING))
            .accessibility_label(tr(keys::Q_CHAT_FORMATTING))
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                // The shortcut is shown next to each item through its
                // action's key binding; the click handler applies the
                // format directly because focus is on the menu, not the
                // composer, when it fires.
                let entries: [(&str, FormatAction, Option<Box<dyn Action>>); 9] = [
                    (
                        tr(keys::Q_CHAT_BOLD),
                        FormatAction::Bold,
                        Some(Box::new(FormatBold)),
                    ),
                    (
                        tr(keys::Q_CHAT_ITALIC),
                        FormatAction::Italic,
                        Some(Box::new(FormatItalic)),
                    ),
                    (
                        tr(keys::Q_CHAT_UNDERLINE),
                        FormatAction::Underline,
                        Some(Box::new(FormatUnderline)),
                    ),
                    (
                        tr(keys::Q_CHAT_STRIKETHROUGH),
                        FormatAction::Strikethrough,
                        Some(Box::new(FormatStrikethrough)),
                    ),
                    (
                        tr(keys::Q_CHAT_INLINE_CODE),
                        FormatAction::Code,
                        Some(Box::new(FormatMonospace)),
                    ),
                    (tr(keys::Q_CHAT_CODE_BLOCK), FormatAction::Pre, None),
                    (
                        tr(keys::MENU_FORMATTING_SPOILER),
                        FormatAction::Spoiler,
                        Some(Box::new(FormatSpoiler)),
                    ),
                    (
                        tr(keys::Q_CHAT_BLOCK_QUOTE),
                        FormatAction::BlockQuote,
                        Some(Box::new(FormatBlockQuote)),
                    ),
                    (
                        tr(keys::Q_CHAT_INSERT_LINK),
                        FormatAction::Link(String::new()),
                        None,
                    ),
                ];
                for (name, action, shortcut) in entries {
                    let owner = owner.clone();
                    let mut item = PopupMenuItem::new(name).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if matches!(action, FormatAction::Link(_))
                                && !this.composer.read(cx).selected_range().is_empty()
                            {
                                // With selected text, ask for the address
                                // (Cmd/Ctrl+K); without, insert `[]()`.
                                this.open_composer_link_dialog(window, cx);
                                return;
                            }
                            this.apply_composer_format(action.clone(), window, cx)
                        });
                    });
                    if let Some(shortcut) = shortcut {
                        item = item.action(shortcut);
                    }
                    menu = menu.item(item);
                }
                let owner = owner.clone();
                menu.separator().item(
                    PopupMenuItem::new(tr(keys::Q_CHAT_CLEAR_FORMATTING))
                        .action(Box::new(FormatClear))
                        .on_click(move |_, window, cx| {
                            let _ =
                                owner.update(cx, |this, cx| this.clear_composer_format(window, cx));
                        }),
                )
            })
    }

    /// What the send options menu shows: silent, previews off, scheduled
    /// and the schedule kind. Read before the menu is built, since it is
    /// built while the app is being updated.
    pub(in crate::ui) fn send_options_state(&self) -> SendOptionsState {
        SendOptionsState {
            silent: self.composer_effective_silent(),
            preview_off: self.pending_send().preview.disabled,
            scheduled: !matches!(self.pending_send().scheduling, ComposerScheduling::None),
            kind: self.schedule_kind(),
        }
    }

    /// Send options for the next message (right-click on Send, like the
    /// official desktop client): silent send, scheduling, link previews.
    pub(in crate::ui) fn send_options_menu(
        owner: WeakEntity<Self>,
        state: SendOptionsState,
        menu: gpui_kit::component::menu::PopupMenu,
    ) -> gpui_kit::component::menu::PopupMenu {
        let SendOptionsState {
            silent,
            preview_off,
            scheduled,
            kind,
        } = state;
        let toggle_silent = owner.clone();
        let schedule = owner.clone();
        let toggle_preview = owner;
        menu.item(
            PopupMenuItem::new(tr(keys::SEND_SILENT_MESSAGE))
                .checked(silent)
                .on_click(move |_, _, cx| {
                    let _ = toggle_silent.update(cx, |this, cx| {
                        this.toggle_composer_silent();
                        cx.notify();
                    });
                }),
        )
        .item(
            PopupMenuItem::new(kind.menu_label(scheduled)).on_click(move |_, window, cx| {
                let _ = schedule.update(cx, |this, cx| {
                    this.open_schedule_picker(ScheduleTarget::Composer, window, cx);
                });
            }),
        )
        .item(
            PopupMenuItem::new(tr(keys::Q_CHAT_LINK_PREVIEW))
                .checked(!preview_off)
                .on_click(move |_, _, cx| {
                    let _ = toggle_preview.update(cx, |this, cx| {
                        this.update_pending_send(|send| {
                            send.preview.disabled = !send.preview.disabled
                        });
                        cx.notify();
                    });
                }),
        )
    }

    /// Chips for send options that differ from the default (silent,
    /// scheduled, previews off) — visible state for an otherwise hidden
    /// menu, each clearable in place — plus the rich-editor entry point.
    /// `None` when there is nothing to show, so the composer stays one row.
    pub(in crate::ui) fn composer_options_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Each chip is built in its own function, so a debug build does not
        // reserve every chip's builder steps at once
        // (docs/decisions/main-thread-stack.md).
        let items: Vec<AnyElement> = [
            self.silent_option_chip(cx),
            self.schedule_option_chip(cx),
            self.preview_option_chip(cx),
            self.rich_editor_entry(cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        if items.is_empty() {
            return None;
        }
        Some(
            div()
                .id("composer-options")
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .px_1()
                .pb_1()
                .children(items)
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn silent_option_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.composer_effective_silent() {
            return None;
        }
        Some(
            option_chip("chip-silent", tr(keys::Q_CHAT_SILENT).into(), cx)
                .child(
                    Button::new("chip-silent-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label(tr(keys::Q_CHAT_SEND_WITH_SOUND))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.composer_effective_silent() {
                                this.toggle_composer_silent();
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn schedule_option_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = match self.pending_send().scheduling {
            ComposerScheduling::None => return None,
            ComposerScheduling::SendAtDate(date) => format!(
                "{} · {}",
                match self.schedule_kind() {
                    ScheduleKind::Reminder => tr(keys::Q_CHAT_REMINDER),
                    ScheduleKind::Schedule => tr(keys::Q_CHAT_SCHEDULED),
                },
                super::message_text::format_unix_date_time(date)
            ),
            ComposerScheduling::SendWhenOnline => tr(keys::Q_CHAT_WHEN_ONLINE).to_string(),
        };
        Some(
            option_chip("chip-schedule", label, cx)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.composer_ui.schedule_popup_open {
                        this.composer_ui.schedule_popup_open = false;
                        cx.notify();
                    } else {
                        this.open_schedule_picker(ScheduleTarget::Composer, window, cx);
                    }
                }))
                .child(
                    Button::new("chip-schedule-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label(tr(keys::CONTEXT_SEND_NOW_MSG))
                        .swallow_press()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.update_pending_send(|send| {
                                send.scheduling = ComposerScheduling::None
                            });
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    #[inline(never)]
    fn preview_option_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pending_send().preview.disabled {
            return None;
        }
        Some(
            option_chip("chip-preview", tr(keys::Q_CHAT_NO_LINK_PREVIEW).into(), cx)
                .child(
                    Button::new("chip-preview-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label(tr(keys::Q_CHAT_SHOW_LINK_PREVIEW))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.update_pending_send(|send| send.preview.disabled = false);
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// M2: the rich editor opens via ⛶ after typing more than 3 lines
    /// (anniversary post). The button hides again while the editor is
    /// open (a ✕ close button takes its place in the editor bar).
    /// Premium gate (`premiumFeatureRichMessages`, schema 1.8.67 line
    /// 8160 — "The ability to send rich messages"): non-Premium users
    /// get the button but tapping it explains the requirement instead
    /// of opening the editor.
    #[inline(never)]
    fn rich_editor_entry(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.composer_ui.rich_editor_open || self.composer.read(cx).value().lines().count() <= 3
        {
            return None;
        }
        Some(
            Button::new("rich-editor-open")
                .label(tr(keys::Q_CHAT_RICH_EDITOR))
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    let premium = this
                        .live
                        .as_ref()
                        .is_some_and(|live| live.driver.session.my_is_premium());
                    if premium {
                        // The editor works on block markup: show it raw.
                        let markup = this.composer_markup(cx);
                        this.composer_ui.rich_editor_open = true;
                        this.set_composer_markup(&markup, window, cx);
                        this.connection.set_note(StatusNote::silent(tr(
                            keys::Q_CHAT_RICH_EDITOR_MARKUP_BECOMES_BLOCKS,
                        )));
                    } else {
                        this.connection.set_note(StatusNote::info(tr(
                            keys::Q_CHAT_RICH_MESSAGES_REQUIRE_TELEGRAM_PREMIUM,
                        )));
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Slice msg-richtext-ai-tools: run one AI action against the open
    /// chat's composer draft. `send` issues the driver request; the
    /// answer (or a TDLib error) lands through the session drain —
    /// never silent, never fake success.
    pub(super) fn run_ai_composer_action(
        &mut self,
        cx: &mut Context<Self>,
        working_note: &str,
        send: impl FnOnce(
            &mut quill::connect::LiveConnect,
            ChatId,
            &str,
        ) -> Result<quill::ids::RequestId, quill::connect::ConnectSendError>,
    ) {
        let text = self.composer_markup(cx);
        if text.trim().is_empty() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_TYPE_SOMETHING_FIRST_THE_AI,
            )));
        } else if let Some(live) = self.live.as_mut()
            && let Some(chat_id) = live.driver.session.open_chat
        {
            self.connection.set_note(match send(live, chat_id, &text) {
                Ok(_) => StatusNote::silent(working_note),
                Err(_) => StatusNote::info(tr(keys::Q_CHAT_AI_TOOLS_UNAVAILABLE_HERE)),
            });
        } else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_CHAT_AI_TOOLS_UNAVAILABLE_HERE)));
        }
        cx.notify();
    }

    /// M2: rich editor bar — block buttons append markup templates to the
    /// composer text; below them a live preview renders the parsed blocks
    /// with the same block renderer as history. The ✕ button closes the
    /// editor (the text stays, so nothing is lost).
    pub(in crate::ui) fn rich_editor_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut buttons = div()
            .id("rich-editor-blocks")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (id, label, template) in [
            ("rich-block-h1", "H1", "# "),
            ("rich-block-h2", "H2", "## "),
            ("rich-block-list", tr(keys::Q_CHAT_LIST), "- "),
            ("rich-block-check", tr(keys::Q_CHAT_CHECK), "[] "),
            ("rich-block-details", tr(keys::Q_CHAT_DETAILS), ">> "),
            ("rich-block-divider", tr(keys::Q_CHAT_DIVIDER), "---\n"),
        ] {
            buttons = buttons.child(Button::new(id).label(label).ghost().on_click(cx.listener(
                move |this, _, window, cx| {
                    this.composer.update(cx, |input, cx| {
                        let mut value = input.value().to_string();
                        if !value.is_empty() && !value.ends_with('\n') {
                            value.push('\n');
                        }
                        value.push_str(template);
                        input.set_value(&value, window, cx);
                    });
                    cx.notify();
                },
            )));
        }
        // Slice msg-richtext-ai-tools: AI actions on the draft. "Fix"
        // runs `fixTextWithAi` (replaces the draft with the fixed text);
        // "Rewrite" runs `composeTextWithAi` with the honest defaults
        // (no translation, current style, no emoji); "Fix rich" and
        // "Rewrite rich" parse the draft to blocks with the same markup
        // parser as the preview and run `fixRichMessageWithAi` /
        // `composeRichMessageWithAi` on them; "Create" treats the
        // draft as the prompt for `createRichMessageWithAi` and the
        // created blocks replace the draft.
        buttons = buttons.child(
            Button::new("rich-ai-fix")
                .label(tr(keys::Q_CHAT_FIX))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        tr(keys::Q_CHAT_AI_FIXING_THE_TEXT),
                        |live, chat_id, text| live.driver.fix_text_with_ai(chat_id, text),
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-rewrite")
                .label(tr(keys::Q_CHAT_REWRITE))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        tr(keys::Q_CHAT_AI_REWRITING),
                        |live, chat_id, text| live.driver.compose_text_with_ai(chat_id, text),
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-create")
                .label(tr(keys::Q_CHAT_CREATE))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        tr(keys::Q_CHAT_AI_CREATING_FROM_THE_PROMPT),
                        |live, chat_id, text| {
                            live.driver.create_rich_message_with_ai(chat_id, text)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-fix-rich")
                .label(tr(keys::Q_CHAT_FIX_RICH))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        tr(keys::Q_CHAT_AI_FIXING_THE_BLOCKS),
                        |live, chat_id, text| {
                            let blocks = quill::rich::preview_blocks(text);
                            live.driver.fix_rich_message_with_ai(chat_id, &blocks)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-rewrite-rich")
                .label(tr(keys::Q_CHAT_REWRITE_RICH))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        tr(keys::Q_CHAT_AI_REWRITING_THE_BLOCKS),
                        |live, chat_id, text| {
                            let blocks = quill::rich::preview_blocks(text);
                            live.driver.compose_rich_message_with_ai(chat_id, &blocks)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-editor-close")
                .label("\u{2715}")
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_rich_editor(window, cx);
                })),
        );
        let bar = div()
            .id("rich-editor-bar")
            .flex()
            .flex_col()
            .gap_1()
            .px_1()
            .py_1()
            .child(buttons);
        bar.children(self.rich_editor_preview(cx))
    }

    /// M1: start a reply to a message from the context menu — the reply
    /// header targets the message; the typed text stays untouched.
    pub(in crate::ui) fn begin_reply_from_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_reply_to(ComposerReplyTo::new(chat_id, message_id), window, cx);
    }

    /// "Reply with timecode": reply to the playing voice message and drop
    /// the player's position into the composer.
    pub(in crate::ui) fn reply_with_timecode(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.playback.active_position_secs() else {
            return;
        };
        let timecode = quill::message_menu::timecode_text(position);
        self.begin_reply_from_message(chat_id, message_id, window, cx);
        self.composer.update(cx, |input, cx| {
            let value = input.value().to_string();
            let cursor = input.selected_range().start.min(value.len());
            let before = value.get(..cursor).unwrap_or(&value);
            let insertion = quill::message_menu::timecode_insertion(before, &timecode);
            input.insert(insertion, window, cx);
        });
    }

    /// M1: unpin every pinned message in the chat (`unpinAllChatMessages`,
    /// TDLib 1.8.67, `schema/td_api.tl:13565`) — the pinned-bar "Unpin
    /// all" action.
    pub(in crate::ui) fn unpin_all_messages(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.unpin_all_chat_messages(chat_id) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_UNPINNING_ALL)),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_UNPIN_ALL)),
                });
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_NO_LIVE_CONNECTION)));
        }
        cx.notify();
    }

    /// M1: retry a failed send (`resendMessages`, TDLib 1.8.67,
    /// `schema/td_api.tl:12251`). Offered only for rows the reducer
    /// marked `failed` **and** `can_retry` — TDLib does not allow every
    /// failed send to be retried. A failed `resendMessages` surfaces in
    /// the status note via `Session::resend_error`.
    pub(in crate::ui) fn retry_failed_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let note = match live.driver.resend_failed_message(chat_id, message_id) {
                Ok(_) => {
                    self.send_started_note(ComposerScheduling::None, tr(keys::Q_CHAT_RETRYING_SEND))
                }
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_RETRY)),
            };
            self.connection.set_note(note);
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_NO_LIVE_CONNECTION)));
        }
        cx.notify();
    }

    /// M1: copy a public share link for a message (`getMessageLink`,
    /// TDLib 1.8.67, `schema/td_api.tl:12064`). The parsed
    /// `messageLink.link` lands in `session.messages.message_link_result`; the
    /// per-frame pump copies it to the clipboard.
    pub(in crate::ui) fn share_message_link(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.get_message_link(chat_id, message_id) {
                    Ok(_) => StatusNote::info(tr(keys::Q_CHAT_FETCHING_MESSAGE_LINK)),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_GET_MESSAGE_LINK)),
                });
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_NO_LIVE_CONNECTION)));
        }
        cx.notify();
    }

    /// Phase B3: label for the picker button (`⏱` cycle affordance).
    pub(in crate::ui) fn self_destruct_button_label(&self) -> String {
        match self.pending_send().self_destruct {
            None => tr(keys::Q_CHAT_OFF).to_string(),
            Some(SelfDestructSend::Timer(secs)) => format!("{secs}s"),
            Some(SelfDestructSend::Immediately) => tr(keys::Q_CHAT_VIEW_ONCE).to_string(),
        }
    }
}

/// A send-option chip of the composer's option row.
fn option_chip(id: &'static str, label: String, cx: &mut Context<QuillApp>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .pl_2()
        .pr_1()
        .h(px(22.))
        .rounded_full()
        .bg(cx.theme().secondary)
        .text_xs()
        .text_color(cx.theme().secondary_foreground)
        .child(label)
}

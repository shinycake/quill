//! Methods moved out of `composer.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Flip silent send for the next message, keeping a chat default
    /// from switching it back on.
    pub(in crate::ui) fn toggle_composer_silent(&mut self) {
        let chat = self.session().and_then(|s| s.open_chat).map(|c| c.0);
        let chat_default =
            chat.is_some_and(|id| self.session().is_some_and(|s| s.sync.is_default_silent(id)));
        if self.composer_effective_silent() {
            self.composer_ui.silent = false;
            if chat_default {
                self.composer_ui.loud_chat = chat;
            }
        } else {
            self.composer_ui.silent = true;
            self.composer_ui.loud_chat = None;
        }
    }

    /// M1: the composer's `messageSendOptions` for the next send.
    pub(in crate::ui) fn composer_send_options(&self) -> SendOptions {
        SendOptions {
            disable_notification: self.composer_effective_silent(),
            scheduling: self.composer_ui.scheduling,
            link_preview_disabled: self.composer_ui.preview_disabled,
            link_preview_above_text: self.composer_ui.preview_above,
            link_preview_media: self.composer_ui.preview_media,
            link_preview_link: self.composer_ui.preview_link,
            // The driver overrides this for secret chats at send time.
            is_secret: false,
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
                Ok(_) => self.connection.status_note = "loading scheduled messages…".into(),
                Err(_) => self.connection.status_note = "could not load scheduled messages".into(),
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
                self.connection.status_note = "scheduled message deleted".into();
            }
            Err(_) => self.connection.status_note = "could not delete scheduled message".into(),
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
            .tooltip("Formatting")
            .accessibility_label("Formatting")
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
                    ("Bold", FormatAction::Bold, Some(Box::new(FormatBold))),
                    ("Italic", FormatAction::Italic, Some(Box::new(FormatItalic))),
                    (
                        "Underline",
                        FormatAction::Underline,
                        Some(Box::new(FormatUnderline)),
                    ),
                    (
                        "Strikethrough",
                        FormatAction::Strikethrough,
                        Some(Box::new(FormatStrikethrough)),
                    ),
                    (
                        "Inline code",
                        FormatAction::Code,
                        Some(Box::new(FormatMonospace)),
                    ),
                    ("Code block", FormatAction::Pre, None),
                    (
                        "Spoiler",
                        FormatAction::Spoiler,
                        Some(Box::new(FormatSpoiler)),
                    ),
                    (
                        "Block quote",
                        FormatAction::BlockQuote,
                        Some(Box::new(FormatBlockQuote)),
                    ),
                    ("Insert link", FormatAction::Link(String::new()), None),
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
                    PopupMenuItem::new("Clear formatting")
                        .action(Box::new(FormatClear))
                        .on_click(move |_, window, cx| {
                            let _ =
                                owner.update(cx, |this, cx| this.clear_composer_format(window, cx));
                        }),
                )
            })
    }

    /// Send options for the next message (right-click on Send, like the
    /// official desktop client): silent send, scheduling, link previews.
    pub(in crate::ui) fn send_options_menu(
        owner: WeakEntity<Self>,
        menu: gpui_kit::component::menu::PopupMenu,
        cx: &App,
    ) -> gpui_kit::component::menu::PopupMenu {
        let Some(app) = owner.upgrade() else {
            return menu;
        };
        let (silent, preview_off, scheduled, kind) = {
            let app = app.read(cx);
            (
                app.composer_effective_silent(),
                app.composer_ui.preview_disabled,
                !matches!(app.composer_ui.scheduling, ComposerScheduling::None),
                app.schedule_kind(),
            )
        };
        let toggle_silent = owner.clone();
        let schedule = owner.clone();
        let toggle_preview = owner;
        menu.item(
            PopupMenuItem::new("Send without sound")
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
            PopupMenuItem::new("Link preview")
                .checked(!preview_off)
                .on_click(move |_, _, cx| {
                    let _ = toggle_preview.update(cx, |this, cx| {
                        this.composer_ui.preview_disabled = !this.composer_ui.preview_disabled;
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
        let chip = |id: &'static str, label: String, cx: &mut Context<Self>| {
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
        };
        let mut row = div()
            .id("composer-options")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .px_1()
            .pb_1();
        let mut any = false;
        if self.composer_effective_silent() {
            any = true;
            row = row.child(
                chip("chip-silent", "Silent".into(), cx).child(
                    Button::new("chip-silent-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label("Send with sound")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.composer_effective_silent() {
                                this.toggle_composer_silent();
                            }
                            cx.notify();
                        })),
                ),
            );
        }
        let schedule_label = match self.composer_ui.scheduling {
            ComposerScheduling::None => None,
            ComposerScheduling::SendAtDate(date) => Some(format!(
                "{} · {}",
                match self.schedule_kind() {
                    ScheduleKind::Reminder => "Reminder",
                    ScheduleKind::Schedule => "Scheduled",
                },
                super::message_text::format_unix_date_time(date)
            )),
            ComposerScheduling::SendWhenOnline => Some("When online".to_string()),
        };
        if let Some(label) = schedule_label {
            any = true;
            row = row.child(
                chip("chip-schedule", label, cx)
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
                            .accessibility_label("Send now")
                            .swallow_press()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.composer_ui.scheduling = ComposerScheduling::None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.composer_ui.preview_disabled {
            any = true;
            row = row.child(
                chip("chip-preview", "No link preview".into(), cx).child(
                    Button::new("chip-preview-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .xsmall()
                        .ghost()
                        .accessibility_label("Show link preview")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.composer_ui.preview_disabled = false;
                            cx.notify();
                        })),
                ),
            );
        }
        // M2: the rich editor opens via ⛶ after typing more than 3 lines
        // (anniversary post). The button hides again while the editor is
        // open (a ✕ close button takes its place in the editor bar).
        // Premium gate (`premiumFeatureRichMessages`, schema 1.8.67 line
        // 8160 — "The ability to send rich messages"): non-Premium users
        // get the button but tapping it explains the requirement instead
        // of opening the editor.
        if !self.composer_ui.rich_editor_open && self.composer.read(cx).value().lines().count() > 3
        {
            row = row.child(
                Button::new("rich-editor-open")
                    .label("⛶ Rich editor")
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
                            this.connection.status_note =
                                "rich editor — markup becomes blocks".into();
                        } else {
                            this.connection.status_note =
                                "Rich messages require Telegram Premium".into();
                        }
                        cx.notify();
                    })),
            );
            any = true;
        }
        any.then(|| row.into_any_element())
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
            self.connection.status_note = "type something first — the AI works on the draft".into();
        } else if let Some(live) = self.live.as_mut()
            && let Some(chat_id) = live.driver.session.open_chat
        {
            self.connection.status_note = match send(live, chat_id, &text) {
                Ok(_) => working_note.into(),
                Err(_) => "AI tools unavailable here".into(),
            };
        } else {
            self.connection.status_note = "AI tools unavailable here".into();
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
            ("rich-block-list", "\u{2022} List", "- "),
            ("rich-block-check", "\u{2611} Check", "[] "),
            ("rich-block-details", "\u{25be} Details", ">> "),
            ("rich-block-divider", "\u{2014} Divider", "---\n"),
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
        buttons = buttons.child(Button::new("rich-ai-fix").label("✨ Fix").ghost().on_click(
            cx.listener(move |this, _, _, cx| {
                this.run_ai_composer_action(cx, "AI fixing the text…", |live, chat_id, text| {
                    live.driver.fix_text_with_ai(chat_id, text)
                });
            }),
        ));
        buttons = buttons.child(
            Button::new("rich-ai-rewrite")
                .label("✨ Rewrite")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(cx, "AI rewriting…", |live, chat_id, text| {
                        live.driver.compose_text_with_ai(chat_id, text)
                    });
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-create")
                .label("✨ Create")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI creating from the prompt…",
                        |live, chat_id, text| {
                            live.driver.create_rich_message_with_ai(chat_id, text)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-fix-rich")
                .label("✨ Fix rich")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI fixing the blocks…",
                        |live, chat_id, text| {
                            let blocks = quill::rich::preview_blocks(text);
                            live.driver.fix_rich_message_with_ai(chat_id, &blocks)
                        },
                    );
                })),
        );
        buttons = buttons.child(
            Button::new("rich-ai-rewrite-rich")
                .label("✨ Rewrite rich")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.run_ai_composer_action(
                        cx,
                        "AI rewriting the blocks…",
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
                    // Back to the formatted field.
                    let markup = this.composer_markup(cx);
                    this.composer_ui.rich_editor_open = false;
                    this.set_composer_markup(&markup, window, cx);
                    cx.notify();
                })),
        );
        let mut bar = div()
            .id("rich-editor-bar")
            .flex()
            .flex_col()
            .gap_1()
            .px_1()
            .py_1()
            .child(buttons);
        // Live preview of the parsed blocks (editor blocks never carry
        // buttons, so the callback ids are unused).
        let text = self.composer.read(cx).value().to_string();
        let blocks = quill::rich::preview_blocks(&text);
        if blocks.iter().any(|block| {
            !matches!(
                block,
                quill::rich::RichBlock::Empty | quill::rich::RichBlock::Unsupported { .. }
            )
        }) {
            let empty: std::collections::HashSet<(i64, u64, u64, bool)> =
                std::collections::HashSet::new();
            let mut preview = div()
                .id("rich-editor-preview")
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(bg_canvas())
                // Cap the preview so a long draft can't squeeze the history
                // to zero height on short windows.
                .max_h(px(320.))
                .overflow_y_scroll();
            for (index, block) in blocks.iter().enumerate() {
                if let Some(child) = rich_block_element(
                    index,
                    block,
                    (0, 0),
                    ChatId(0),
                    MessageId(0),
                    &empty,
                    // Settings → Appearance: message font size.
                    self.msg_font(),
                    cx,
                ) {
                    preview = preview.child(child);
                }
            }
            bar = bar.child(preview);
        }
        bar
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
        let preview = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .map(effective_preview)
            .unwrap_or_default();
        self.begin_reply_to(
            ComposerReplyTo::new(chat_id, message_id, preview),
            window,
            cx,
        );
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
        let Some(position) = self.playback.clock.as_ref().map(|c| c.elapsed_secs()) else {
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
            self.connection.status_note = match live.driver.unpin_all_chat_messages(chat_id) {
                Ok(_) => "unpinning all…".into(),
                Err(_) => "could not unpin all".into(),
            };
        } else {
            self.connection.status_note = "no live connection".into();
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
            self.connection.status_note =
                match live.driver.resend_failed_message(chat_id, message_id) {
                    Ok(_) => self.send_started_note(ComposerScheduling::None, "retrying send…"),
                    Err(_) => "could not retry".into(),
                };
        } else {
            self.connection.status_note = "no live connection".into();
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
            self.connection.status_note = match live.driver.get_message_link(chat_id, message_id) {
                Ok(_) => "fetching message link…".into(),
                Err(_) => "could not get message link".into(),
            };
        } else {
            self.connection.status_note = "no live connection".into();
        }
        cx.notify();
    }

    /// Phase B3: label for the picker button (`⏱` cycle affordance).
    pub(in crate::ui) fn self_destruct_button_label(&self) -> String {
        match self.composer_ui.self_destruct {
            None => "Off".to_string(),
            Some(SelfDestructSend::Timer(secs)) => format!("{secs}s"),
            Some(SelfDestructSend::Immediately) => "View once".to_string(),
        }
    }

    /// Phase 3.3: recompute the `/` command menu from the composer text.
    /// Called on every composer event (`Change` path) and after
    /// programmatic `set_value` writes, which suppress `Change`. Opens
    /// when the text ends with a `/`-led token at a word boundary and the
    /// open chat's bot has commands; closes otherwise (non-bot chat, no
    /// commands, invalid trigger, empty composer).
    pub(in crate::ui) fn sync_command_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        // Bots slice: the `@bot` inline trigger owns the composer start —
        // the `/` menu never competes with the inline-results dropdown.
        let inline_active = quill::composer::inline_query_trigger(&text).is_some();
        let triggered = command_menu_trigger(&text).is_some() && !inline_active;
        let open_chat = self.session().and_then(|session| session.open_chat);
        let has_items = open_chat.is_some_and(|chat_id| {
            self.session()
                .map(|session| !session.command_menu_items(chat_id).is_empty())
                .unwrap_or(false)
        });
        let open = triggered && has_items;
        if open == self.composer_ui.command_menu_open {
            return;
        }
        self.composer_ui.command_menu_open = open;
        self.composer_ui.command_menu_selected = 0;
        cx.notify();
    }

    /// Phase 3.3: Esc / blur / selection / chat-switch dismissal. Returns
    /// true when the menu was open (the key interceptor swallows the
    /// keystroke only then).
    pub(in crate::ui) fn close_command_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.composer_ui.command_menu_open {
            return false;
        }
        self.composer_ui.command_menu_open = false;
        self.composer_ui.command_menu_selected = 0;
        cx.notify();
        true
    }

    /// Phase 3.3: Up/Down highlight. Returns true when the menu consumed
    /// the key (open with rows to move between); the selection wraps.
    pub(in crate::ui) fn step_command_menu(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = self
            .command_menu_state(cx)
            .map(|(_, items)| items.len())
            .unwrap_or(0);
        if !self.composer_ui.command_menu_open || rows == 0 {
            return false;
        }
        self.composer_ui.command_menu_selected = (self.composer_ui.command_menu_selected as i32
            + delta)
            .rem_euclid(rows as i32) as usize;
        cx.notify();
        true
    }

    /// Phase 3.3: current menu rows — (typed prefix, prefix-filtered
    /// items). `None` when the menu is closed, the composer has no `/`
    /// trigger, the open chat's bot has no commands, or nothing matches.
    pub(in crate::ui) fn command_menu_state(
        &self,
        cx: &Context<Self>,
    ) -> Option<(String, Vec<CommandMenuItem>)> {
        if !self.composer_ui.command_menu_open {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let prefix = command_menu_trigger(&text)?;
        let chat_id = self.session()?.open_chat?;
        let items = self.session()?.command_menu_items(chat_id);
        if items.is_empty() {
            return None;
        }
        let filtered: Vec<CommandMenuItem> = filter_command_menu_items(&items, prefix)
            .into_iter()
            .cloned()
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some((prefix.to_string(), filtered))
    }

    /// Phase 3.3: Enter with the menu open picks the highlighted row.
    /// Returns true when Enter was consumed.
    pub(in crate::ui) fn pick_command_menu_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        if !self.composer_ui.command_menu_open || items.is_empty() {
            return false;
        }
        let index = self.composer_ui.command_menu_selected.min(items.len() - 1);
        self.pick_command_menu_index(index, window, cx);
        true
    }

    /// Phase 3.3: tap / Enter pick. The partial `/`-token is replaced via
    /// the 3.1 insert helper, then the menu closes (the next `Change`
    /// reopens it if a trigger token remains).
    pub(in crate::ui) fn pick_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let command = item.command.clone();
        let current = self.composer_markup(cx);
        let base = strip_command_menu_trigger(&current).unwrap_or(current.as_str());
        // Trailing space (tdesktop behavior): without it, the `/`-token
        // trigger still matches `/command`, the menu reopens on the next
        // Enter and consumes it in a no-op loop — Enter could never send.
        let next = format!(
            "{} ",
            quill::composer::insert_bot_command_text(base, &command).trim_end()
        );
        self.set_composer_markup(&next, window, cx);
        self.composer.update(cx, |input, cx| {
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
        });
        self.close_command_menu(cx);
    }

    /// A clicked command is sent at once (Telegram Desktop's bot command
    /// list): the composer's `/`-token is dropped, any other text stays.
    pub(in crate::ui) fn send_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let command = format!("/{}", item.command.trim_start_matches('/'));
        let current = self.composer_markup(cx);
        let rest = strip_command_menu_trigger(&current)
            .unwrap_or(current.as_str())
            .to_string();
        self.close_command_menu(cx);
        self.set_composer_markup(rest.trim_end(), window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.submit_composer(command, window, cx);
    }
}

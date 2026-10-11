//! Methods moved out of `translate_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(super) fn translate_bar_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let model = self.translate_bar_model(chat_id)?;
        let label = bar_label_for(model.translated, model.to, model.automatic, model.from);
        let to = model.to;
        let from = model.from;
        let owner = cx.entity().downgrade();
        let theme = cx.theme();
        let border = theme.border;
        let menu = Button::new("translate-bar-menu")
            .icon(gpui_kit::assets::IconName::EllipsisVertical)
            .ghost()
            .small()
            .accessibility_label("Translation options")
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                let chooser = owner.clone();
                menu = menu
                    .item(
                        PopupMenuItem::new(format!("Translate To: {}", language_name(to)))
                            .on_click(move |_, window, cx| {
                                let _ = chooser.update(cx, |this, cx| {
                                    this.open_translate_chooser(to, window, cx);
                                });
                            }),
                    )
                    .separator();
                if let Some(from) = from {
                    let skip = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("Don't translate {}", language_name(from)))
                            .on_click(move |_, _, cx| {
                                let _ = skip.update(cx, |this, cx| {
                                    this.skip_translating(chat_id, from, cx);
                                });
                            }),
                    );
                }
                let hide = owner.clone();
                menu.item(PopupMenuItem::new("Hide").on_click(move |_, _, cx| {
                    let _ = hide.update(cx, |this, cx| this.hide_translate_bar(chat_id, cx));
                }))
            });
        Some(
            div()
                .id("translate-bar")
                .flex()
                .flex_none()
                .items_center()
                .min_h(px(40.))
                .border_b_1()
                .border_color(border)
                .bg(bg_canvas())
                .child(
                    div()
                        .id("translate-bar-toggle")
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(label.clone())
                        .tab_index(0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_chat_translation(chat_id, cx);
                        }))
                        .child(
                            Icon::new(gpui_kit::assets::IconName::Languages)
                                .size(px(16.))
                                .text_color(accent()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(accent())
                                .truncate()
                                .child(label),
                        ),
                )
                .child(div().flex_none().px_1().child(menu))
                .into_any_element(),
        )
    }

    pub(super) fn translate_toast_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let toast = self
            .settings
            .translate
            .toast
            .as_ref()
            .filter(|toast| toast.chat_id == chat_id)?
            .clone();
        let action = toast.action;
        Some(
            div()
                .id("translate-toast")
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .min_h(px(40.))
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(div().flex_1().min_w_0().text_sm().child(toast.text.clone()))
                .child(
                    Button::new("translate-toast-action")
                        .label(toast.label)
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.settings.translate.toast = None;
                            match action {
                                ToastAction::ShowBar => {
                                    this.set_translate_bar_hidden(chat_id, false, cx)
                                }
                                ToastAction::OpenSkipList => {
                                    this.open_translate_skip_list(window, cx);
                                }
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn show_translate_toast(
        &mut self,
        chat_id: ChatId,
        text: String,
        label: &'static str,
        action: ToastAction,
        cx: &mut Context<Self>,
    ) {
        self.settings.translate.toast_seq += 1;
        let id = self.settings.translate.toast_seq;
        self.settings.translate.toast = Some(TranslateToast {
            chat_id,
            text,
            label,
            action,
            id,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .settings
                    .translate
                    .toast
                    .as_ref()
                    .is_some_and(|t| t.id == id)
                {
                    this.settings.translate.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Click on the bar: translate the chat, or show it in the original.
    pub(super) fn toggle_chat_translation(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(model) = self.translate_bar_model(chat_id) else {
            return;
        };
        let to = (!model.translated).then_some(model.to);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.set_chat_translated_to(chat_id, to);
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.set_chat_translated_to(chat_id, to);
        }
        cx.notify();
    }

    /// Bar menu → Don't translate {language}.
    pub(super) fn skip_translating(
        &mut self,
        chat_id: ChatId,
        language: &'static str,
        cx: &mut Context<Self>,
    ) {
        let ui = self.translate_ui_language();
        self.set_translate_prefs(cx, |prefs| prefs.add_skip(language, &ui));
        self.show_translate_toast(
            chat_id,
            format!(
                "{} added to the Do Not Translate list.",
                language_name(language)
            ),
            "Settings",
            ToastAction::OpenSkipList,
            cx,
        );
    }

    /// Hide or show the chat's translate bar. A live account sends
    /// `toggleChatIsTranslatable` (the server flag, as in tdesktop); the
    /// demo keeps the choice locally.
    pub(super) fn set_translate_bar_hidden(
        &mut self,
        chat_id: ChatId,
        hidden: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .toggle_chat_is_translatable(chat_id, !hidden)
                .is_ok()
        {
            self.set_translate_prefs(cx, |p| p.set_bar_hidden(chat_id.0, false));
            return;
        }
        self.set_translate_prefs(cx, |p| p.set_bar_hidden(chat_id.0, hidden));
    }

    /// Bar menu → Hide.
    pub(super) fn hide_translate_bar(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let kind = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.kind.clone());
        let phrase = match kind {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => "Translation bar is now hidden for this channel.",
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. }) | None => {
                "Translation bar hidden for this chat."
            }
            _ => "Translation bar is now hidden for this group.",
        };
        self.set_translate_bar_hidden(chat_id, true, cx);
        self.show_translate_toast(
            chat_id,
            phrase.to_string(),
            "Undo",
            ToastAction::ShowBar,
            cx,
        );
    }

    /// Show the open chat's translated messages in place of the originals.
    pub(in crate::ui) fn apply_chat_translation(
        &self,
        chat_id: ChatId,
        messages: &mut [HistoryMessage],
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(to) = session.chat_translated_to(chat_id) else {
            return;
        };
        let automatic = session.chat_auto_translate(chat_id);
        for message in messages.iter_mut().filter(|m| automatic || !m.is_outgoing) {
            if let Some(Translation::Done { text, entities }) =
                session.message_translation(chat_id, message.id, to)
            {
                replace_content_text(&mut message.content, text, entities);
            }
        }
    }

    /// The revision the history rows' cache must include.
    pub(in crate::ui) fn translate_revision(&self) -> u64 {
        self.session().map_or(0, |s| s.messages.translate.revision)
    }

    /// Per-tick work: keep the open chat's translation going (request what
    /// is missing, follow a changed target language, stop when the bar is
    /// no longer offered). Returns whether anything changed.
    pub(in crate::ui) fn pump_translation(&mut self) -> bool {
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let Some(current) = self
            .session()
            .and_then(|s| s.chat_translated_to(chat_id).map(str::to_string))
        else {
            return false;
        };
        let Some(model) = self.translate_bar_model(chat_id) else {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.set_chat_translated_to(chat_id, None);
            }
            return true;
        };
        if current != model.to {
            if let Some(live) = self.live.as_mut() {
                live.driver
                    .session
                    .set_chat_translated_to(chat_id, Some(model.to));
            }
            return true;
        }
        let ui = self.translate_ui_language();
        let skip = self.settings.translate.prefs.skip(&ui);
        let wanted: Vec<MessageId> = {
            let Some(session) = self.session() else {
                return false;
            };
            let Some(history) = session.histories.get(&chat_id.0) else {
                return false;
            };
            let room = MAX_IN_FLIGHT.saturating_sub(session.messages.translate.jobs.len());
            history
                .ordered()
                .into_iter()
                .rev()
                .filter(|m| (model.automatic || !m.is_outgoing) && !m.pending && m.id.0 > 0)
                .filter(|m| {
                    translatable_content(&m.content).is_some_and(|(text, _)| {
                        detect_language(text)
                            .is_none_or(|lang| lang != model.to && !skip.contains(&lang))
                            && text.chars().any(char::is_alphabetic)
                    })
                })
                .filter(|m| session.needs_translation(chat_id, m.id, model.to))
                .take(room)
                .map(|m| m.id)
                .collect()
        };
        let mut sent = false;
        if let Some(live) = self.live.as_mut() {
            for id in wanted {
                sent |= live.driver.translate_message(chat_id, id, model.to).is_ok();
            }
        }
        sent
    }

    /// Settings → Translation: Show Translate Button, Translate Entire Chats
    /// (Premium), Do Not Translate.
    pub(in crate::ui) fn translate_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = &self.settings.translate.prefs;
        let premium = self.session().is_some_and(Session::is_premium);
        let ui = self.translate_ui_language();
        let skip = prefs.skip(&ui);
        let skip_label = if skip.len() > 1 {
            format!("{} languages", skip.len())
        } else {
            skip.first()
                .map(|code| language_name(code))
                .unwrap_or_default()
        };
        let switch_row = |title: &'static str, hint: SharedString, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().child(title))
                        .child(muted_text(hint, cx)),
                )
                .child(control)
        };
        let show = switch_row(
            "Show Translate Button",
            "The \"Translate\" button will appear in the context menu of messages containing text."
                .into(),
            Switch::new("translate-show-button")
                .checked(prefs.show_button)
                .accessibility_label("Show Translate Button")
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.set_translate_prefs(cx, |p| p.show_button = on);
                }))
                .into_any_element(),
        );
        let chats = switch_row(
            "Translate Entire Chats",
            if premium {
                "Offer to translate whole conversations with a bar above the chat.".into()
            } else {
                "Translating entire chats requires Telegram Premium.".into()
            },
            Switch::new("translate-chats")
                .checked(prefs.translate_chats && premium)
                .disabled(!premium)
                .accessibility_label("Translate Entire Chats")
                .on_click(cx.listener(|this, &on, _, cx| {
                    if !this.session().is_some_and(Session::is_premium) {
                        this.connection.status_note =
                            "Translating entire chats requires Telegram Premium".into();
                        cx.notify();
                        return;
                    }
                    this.set_translate_prefs(cx, |p| p.translate_chats = on);
                }))
                .into_any_element(),
        );
        let skip_row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(div().text_sm().child("Do Not Translate"))
            .child(
                Button::new("translate-skip-languages")
                    .label(skip_label)
                    .ghost()
                    .small()
                    .accessibility_label("Choose languages not to translate")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_translate_skip_list(window, cx);
                    })),
            );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_semibold().text_sm().child("Translate Messages"))
            .child(show)
            .child(chats)
            .when(prefs.show_button || prefs.translate_chats, |column| {
                column.child(skip_row)
            })
            .into_any_element()
    }
}

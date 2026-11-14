//! Methods moved out of `translate_ui.rs` to keep files under 1000 lines.

use super::pump_scan::PumpKey;
use super::*;
use crate::ui::settings_window::rows::{group, link_row, switch_row};
use quill::lang::{keys, tr, tr_with, trn};

impl QuillApp {
    pub(super) fn translate_bar_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.translate_bar_model(chat_id)?;
        self.translate_bar_view_body(chat_id, cx)
    }

    /// `translate_bar_view` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn translate_bar_view_body(
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
            .accessibility_label(tr(keys::Q_REST_TRANSLATION_OPTIONS))
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
                        PopupMenuItem::new(tr_with(
                            keys::Q_REST_TRANSLATE_TO,
                            &[("language_name", &(language_name(to)).to_string())],
                        ))
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
                        PopupMenuItem::new(tr_with(
                            keys::TRANSLATE_MENU_DONT,
                            &[("name", &(language_name(from)).to_string())],
                        ))
                        .on_click(move |_, _, cx| {
                            let _ = skip.update(cx, |this, cx| {
                                this.skip_translating(chat_id, from, cx);
                            });
                        }),
                    );
                }
                let hide = owner.clone();
                menu.item(PopupMenuItem::new(tr(keys::TRANSLATE_MENU_HIDE)).on_click(
                    move |_, _, cx| {
                        let _ = hide.update(cx, |this, cx| this.hide_translate_bar(chat_id, cx));
                    },
                ))
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
        self.translate
            .toast
            .toast()
            .filter(|toast| toast.chat_id == chat_id)?;
        self.translate_toast_view_body(chat_id, cx)
    }

    /// `translate_toast_view` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn translate_toast_view_body(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let toast = self
            .translate
            .toast
            .toast()
            .filter(|toast| toast.chat_id == chat_id)?;
        let (text, label, action) = (toast.text.clone(), toast.label, toast.action);
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
                .child(div().flex_1().min_w_0().text_sm().child(text))
                .child(
                    Button::new("translate-toast-action")
                        .label(label)
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.translate.toast = ToastStage::None;
                            match action {
                                ToastAction::ShowBar => {
                                    // Undone: the Hide's answer no longer matters.
                                    this.forget_hide();
                                    this.set_translate_bar_hidden(chat_id, false, cx);
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
        // The toast is the screen's: its timer lives in the stage, so an
        // account switch or a newer toast drops it.
        let hide = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                this.translate.toast = ToastStage::None;
                cx.notify();
            });
        });
        self.translate.toast = ToastStage::Shown {
            toast: TranslateToast {
                chat_id,
                text,
                label,
                action,
            },
            _hide: hide,
        };
        cx.notify();
    }

    /// Click on the bar: translate the chat, or show it in the original.
    pub(super) fn toggle_chat_translation(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(model) = self.translate_bar_model(chat_id) else {
            return;
        };
        let to = (!model.translated).then_some(model.to);
        if to.is_some() {
            // Turning translation on starts a new scan, which asks once more
            // for what failed before (as showing the chat again does).
            self.translate.pump.remove(&chat_id.0);
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.set_chat_translated_to(chat_id, to);
            quill::wake::UPDATES.poke();
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
            tr_with(
                keys::TRANSLATE_DONT_ADDED,
                &[("name", &(language_name(language)).to_string())],
            ),
            tr(keys::MENU_SETTINGS),
            ToastAction::OpenSkipList,
            cx,
        );
    }

    /// Hide or show the chat's translate bar. A live account sends
    /// `toggleChatIsTranslatable` (the server flag, as in tdesktop); the
    /// demo keeps the choice locally.
    /// The request that went out, for a live account.
    pub(in crate::ui) fn set_translate_bar_hidden(
        &mut self,
        chat_id: ChatId,
        hidden: bool,
        cx: &mut Context<Self>,
    ) -> Option<RequestId> {
        let account = self
            .session()
            .map_or_else(quill::ids::AccountKey::primary, |s| s.account.clone());
        let sent = self.live.as_mut().and_then(|live| {
            live.driver
                .toggle_chat_is_translatable(chat_id, !hidden)
                .ok()
        });
        if sent.is_some() {
            self.set_translate_prefs(cx, |p| p.set_bar_hidden(&account, chat_id.0, false));
            return sent;
        }
        self.set_translate_prefs(cx, |p| p.set_bar_hidden(&account, chat_id.0, hidden));
        None
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
            }) => tr(keys::TRANSLATE_HIDDEN_CHANNEL),
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. }) | None => {
                tr(keys::TRANSLATE_HIDDEN_USER)
            }
            _ => tr(keys::TRANSLATE_HIDDEN_GROUP),
        };
        let sent = self.set_translate_bar_hidden(chat_id, true, cx);
        self.await_hide(sent);
        self.show_translate_toast(
            chat_id,
            phrase.to_string(),
            tr(keys::TRANSLATE_UNDO),
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
        let refused = self.end_refused_hide();
        self.pump_translation_pass() || refused
    }

    /// The answer to the Hide the bar sent: a refusal (TDLib put the bar
    /// back, `RequestRollback::ChatIsTranslatable`) ends the "hidden, Undo"
    /// toast if it is still up, and the note says the hide failed; a done
    /// or a lost request just ends the wait. True when a refusal ended it.
    fn end_refused_hide(&mut self) -> bool {
        let HideStage::Sent(request) = self.translate.hide else {
            return false;
        };
        let answer = match self.session_mut() {
            Some(session) => match session.acks.take(request) {
                Some(answer) => Some(answer),
                None if session.requests.get(request).is_none() => {
                    // Left flight with no answer (dropped by the stale
                    // sweep or a reconnect): nobody is left to tell.
                    session.acks.forget(request);
                    Some(quill::state::Answer::Done)
                }
                None => return false,
            },
            None => Some(quill::state::Answer::Done),
        };
        self.translate.hide = HideStage::Idle;
        if !matches!(answer, Some(quill::state::Answer::Failed(_))) {
            return false;
        }
        if self
            .translate
            .toast
            .toast()
            .is_some_and(|toast| matches!(toast.action, ToastAction::ShowBar))
        {
            self.translate.toast = ToastStage::None;
        }
        self.set_note(StatusNote::error(tr(keys::Q_TRANSLATE_HIDE_FAILED)));
        true
    }

    /// The Hide's request is no longer waited on (Undo, or a newer Hide):
    /// forgotten, so its answer is not kept for nobody.
    pub(in crate::ui) fn forget_hide(&mut self) {
        if let HideStage::Sent(old) = std::mem::take(&mut self.translate.hide)
            && let Some(session) = self.session_mut()
        {
            session.acks.forget(old);
        }
    }

    /// Wait for the answer to the Hide just sent as `request`.
    fn await_hide(&mut self, request: Option<RequestId>) {
        self.forget_hide();
        if let Some(request) = request
            && let Some(session) = self.session_mut()
        {
            session.acks.expect(request);
            self.translate.hide = HideStage::Sent(request);
        }
    }

    fn pump_translation_pass(&mut self) -> bool {
        // Only the chats some window shows keep their scan. This runs before
        // every early return: a chat that was closed, or left for one that
        // is not translated, must lose its scan, or coming back to it would
        // not count as shown again and its failures would never be retried.
        let gone: Vec<i64> = self
            .translate
            .pump
            .keys()
            .copied()
            .filter(|chat| {
                !self
                    .session()
                    .is_some_and(|session| session.chat_is_shown(ChatId(*chat)))
            })
            .collect();
        for chat in gone {
            self.translate.pump.remove(&chat);
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat_id()) else {
            return false;
        };
        let Some(current) = self
            .session()
            .and_then(|s| s.chat_translated_to(chat_id).map(str::to_string))
        else {
            return false;
        };
        let Some(model) = self.translate_bar_model(chat_id) else {
            // Not offered any more: the chat is shown in the original (the
            // demo session too, or this reports a change on every tick).
            match (self.live.as_mut(), self.demo_session.as_mut()) {
                (Some(live), _) => live.driver.session.set_chat_translated_to(chat_id, None),
                (None, Some(demo)) => demo.set_chat_translated_to(chat_id, None),
                (None, None) => return false,
            }
            return true;
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.fail_orphaned_translations();
        }
        let followed = current != model.to;
        if followed && let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .set_chat_translated_to(chat_id, Some(model.to));
        }
        let ui = self.translate_ui_language();
        let skip = self.preferences.translate.prefs.skip(&ui);
        // The scan is taken only once there is a history to walk, so an
        // early return keeps what it learned.
        if !self
            .session()
            .is_some_and(|s| s.histories.contains_key(&chat_id.0))
        {
            return followed;
        }
        // A scan that is new is a chat shown again: what failed before is
        // asked for once more (not on every pass, which would hammer a
        // connection that is down).
        let removed = self.translate.pump.remove(&chat_id.0);
        let reopened = removed.is_none();
        let mut scan = removed.unwrap_or_default();
        let retried = reopened
            && self
                .session_mut()
                .is_some_and(|session| session.retry_failed_translations(chat_id));
        let wanted: Vec<MessageId> = {
            let Some(session) = self.session() else {
                self.translate.pump.insert(chat_id.0, scan);
                return followed || retried;
            };
            let Some(history) = session.histories.get(&chat_id.0) else {
                self.translate.pump.insert(chat_id.0, scan);
                return followed || retried;
            };
            let room = MAX_IN_FLIGHT.saturating_sub(session.messages.translate.in_flight());
            let key = PumpKey {
                chat: chat_id.0,
                to: model.to,
                automatic: model.automatic,
                skip: skip.to_vec(),
            };
            let revision = history.messages.revision();
            // Only the messages that wanted a translation, or were stored
            // or changed since the last pass (`pump_scan`).
            let found: Vec<MessageId> = scan
                .candidates(
                    &key,
                    || history.messages.keys().rev().copied().collect(),
                    |since| history.messages.changed_since(since),
                )
                .into_iter()
                .filter_map(|id| history.messages.get(&id))
                .filter(|m| (model.automatic || !m.is_outgoing) && !m.pending() && m.id.0 > 0)
                .filter(|m| {
                    translatable_content(&m.content).is_some_and(|(text, _)| {
                        detect_language(text)
                            .is_none_or(|lang| lang != model.to && !skip.contains(&lang))
                            && text.chars().any(char::is_alphabetic)
                    })
                })
                .filter(|m| session.translation_unsettled(chat_id, m.id, model.to))
                .map(|m| m.id)
                .collect();
            // The scan keeps every unsettled message, failed ones that ran
            // out of attempts included: a reconnect gives them back.
            scan.record(key, revision, found.iter().map(|id| id.0).collect());
            found
                .into_iter()
                .filter(|id| session.needs_translation(chat_id, *id, model.to))
                .take(room)
                .collect()
        };
        self.translate.pump.insert(chat_id.0, scan);
        let Some(live) = self.live.as_mut() else {
            #[cfg(test)]
            self.translate
                .asked
                .extend(wanted.into_iter().map(|id| (id, model.to)));
            return followed || retried;
        };
        let mut sent = false;
        for id in wanted {
            sent |= live.driver.translate_message(chat_id, id, model.to).is_ok();
        }
        followed || sent || retried
    }

    /// Settings → Translation: Show Translate Button, Translate Entire Chats
    /// (Premium), Do Not Translate.
    pub(in crate::ui) fn translate_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = &self.preferences.translate.prefs;
        let premium = self.session().is_some_and(Session::is_premium);
        let ui = self.translate_ui_language();
        let skip = prefs.skip(&ui);
        let skip_label: String = if skip.len() > 1 {
            trn(
                keys::LANGUAGES_COUNT,
                i64::try_from(skip.len()).unwrap_or(i64::MAX),
            )
        } else {
            skip.first()
                .map(|code| language_name(code))
                .unwrap_or_default()
        };
        let skip_shown = prefs.show_button || prefs.translate_chats;
        let show = switch_row(
            "translate-show-button",
            tr(keys::TRANSLATE_SETTINGS_SHOW),
            Some(tr(keys::TRANSLATE_SETTINGS_ABOUT)),
            prefs.show_button,
            cx.listener(|this, &on, _, cx| {
                this.set_translate_prefs(cx, |p| p.show_button = on);
            }),
            cx,
        );
        let show = self.setting_spot(
            &["language.translate_button"],
            self.setting_stand_in(
                !skip_shown,
                &["language.do_not_translate"],
                show.into_any_element(),
                cx,
            ),
            cx,
        );
        let chats = switch_row(
            "translate-chats",
            tr(keys::TRANSLATE_SETTINGS_CHAT),
            Some(if premium {
                tr(keys::Q_TRANSLATE_CHATS_HINT)
            } else {
                tr(keys::Q_TRANSLATE_CHATS_PREMIUM)
            }),
            prefs.translate_chats && premium,
            cx.listener(|this, &on, _, cx| {
                // tdesktop `ShowPremiumPreviewToBuy(RealTimeTranslation)`.
                if !this.session().is_some_and(Session::is_premium) {
                    this.open_premium_to_buy(
                        quill::premium_hub::PremiumFeature::RealTimeChatTranslation,
                        cx,
                    );
                    return;
                }
                this.set_translate_prefs(cx, |p| p.translate_chats = on);
            }),
            cx,
        );
        let chats = self.setting_spot(&["language.translate_chats"], chats.into_any_element(), cx);
        let skip_row = link_row(
            "translate-skip-languages",
            tr(keys::TRANSLATE_SETTINGS_CHOOSE),
            Some(skip_label.into()),
            cx.listener(|this, _, window, cx| this.open_translate_skip_list(window, cx)),
            cx,
        );
        let to_row = link_row(
            "translate-to-language",
            tr(keys::TRANSLATE_MENU_TO),
            Some(quill::translate::language_native_name(prefs.to_language(&ui)).into()),
            cx.listener(|this, _, window, cx| this.open_translate_to_setting(window, cx)),
            cx,
        );
        let mut rows = vec![show, chats];
        if prefs.show_button {
            rows.push(to_row);
        }
        if skip_shown {
            rows.push(self.setting_spot(&["language.do_not_translate"], skip_row, cx));
        }
        group(Some(tr(keys::TRANSLATE_SETTINGS_SUBTITLE)), rows).into_any_element()
    }
}

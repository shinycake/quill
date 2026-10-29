//! Methods moved out of `appearance.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The custom accent field's state, showing the old single
    /// `accent_rgb` when there is one (else empty, with its placeholder).
    /// A committed color is limited like tdesktop's
    /// (`limit_custom_accent`) and becomes the accent of the built-in
    /// theme on screen.
    pub(in crate::ui) fn new_accent_picker(
        accent_rgb: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        let picker = cx.new(|cx| {
            let state = ColorPickerState::new(window, cx);
            if accent_rgb == 0 {
                state
            } else {
                state.default_value(Hsla::from(rgb(accent_rgb)))
            }
        });
        cx.subscribe_in(
            &picker,
            window,
            |this, picker, event: &ColorPickerEvent, window, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                let limited = limit_custom_accent(*color, cx.theme().is_dark());
                if limited != *color {
                    picker.update(cx, |picker, cx| picker.set_value(limited, window, cx));
                }
                let value = accent_rgb_from(limited);
                // The accent of the built-in theme on screen (custom
                // themes take no accent, as in tdesktop).
                if let Some(theme) = this.applied_builtin_theme() {
                    this.set_theme_accent(theme, value, cx);
                }
            },
        )
        .detach();
        picker
    }

    /// tdesktop's "Interface scale" (Settings > Chat settings). GPUI has no
    /// window-wide scale factor, so this scales the rem size: text and
    /// rem-based spacing follow, fixed-pixel widths (avatars, chat rows) do
    /// not — the hint says so.
    pub(in crate::ui) fn appearance_scale_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.appearance.interface_scale_pct;
        let mut row = div().flex().flex_wrap().gap_2().items_center();
        for pct in quill::settings::INTERFACE_SCALE_CHOICES {
            row = row.child(self.appearance_chip(
                format!("appearance-scale-{pct}"),
                format!("{pct}%"),
                current == pct,
                cx,
                move |this, cx| this.set_appearance(cx, |a| a.interface_scale_pct = pct),
            ));
        }
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_INTERFACE_SCALE),
            tr(keys::Q_PREFS_SCALES_THE_WHOLE_INTERFACE_APPLIED),
            row.into_any_element(),
        )
    }

    pub(in crate::ui) fn appearance_wallpaper_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .appearance
            .wallpaper_rgb
            .filter(|_| !self.appearance.telegram_wallpaper);
        let mut row = div()
            .flex()
            .gap_2()
            .items_center()
            .child(self.appearance_chip(
                "appearance-wallpaper-default",
                tr(keys::Q_PREFS_DEFAULT),
                current.is_none(),
                cx,
                |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.wallpaper_rgb = None;
                        a.telegram_wallpaper = false;
                    })
                },
            ));
        for &(color, name) in WALLPAPER_PRESETS {
            row = row.child(self.appearance_swatch(
                format!("appearance-wallpaper-{color:06x}"),
                color,
                tr(name),
                current == Some(color),
                cx,
                move |this, cx| {
                    this.set_appearance(cx, |a| {
                        a.wallpaper_rgb = Some(color);
                        a.telegram_wallpaper = false;
                    })
                },
            ));
        }
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_CHAT_WALLPAPER),
            tr(keys::Q_PREFS_SOLID_COLOR_BEHIND_THE_MESSAGE),
            row.into_any_element(),
        )
    }

    pub(in crate::ui) fn appearance_font_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let size = self.appearance.font_size_px;
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("appearance-font-down")
                    .label(tr(keys::Q_PREFS_A))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.bump_font_size(cx, -1))),
            )
            .child(
                div()
                    .text_sm()
                    .min_w(px(56.))
                    .text_center()
                    .child(format!("{size}px")),
            )
            .child(
                Button::new("appearance-font-up")
                    .label(tr(keys::Q_PREFS_A_2))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.bump_font_size(cx, 1))),
            );
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_MESSAGE_TEXT_SIZE),
            tr(keys::Q_PREFS_CHAT_MESSAGE_TEXT_12_20),
            control.into_any_element(),
        )
    }

    pub(in crate::ui) fn appearance_bubble_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let bubbles = self.appearance.bubbles;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-style")
            .selected_index(Some(if bubbles { 0 } else { 1 }))
            .children([
                Radio::new("appearance-style-bubbles").label(tr(keys::Q_PREFS_BUBBLES)),
                Radio::new("appearance-style-plain").label(tr(keys::Q_PREFS_PLAIN)),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.bubbles = ix == 0);
            }));
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_CHAT_STYLE),
            tr(keys::Q_PREFS_BUBBLES_OR_PLAIN_ROWS_WITHOUT),
            control.into_any_element(),
        )
    }

    /// A labeled toggle row: title + hint on the left, kit `Switch` on
    /// the right (the data-storage dialog pattern).
    pub(in crate::ui) fn appearance_switch_row(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        title: &str,
        hint: &str,
        checked: bool,
        on_change: impl Fn(&mut AppearancePrefs, bool) + 'static,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(title.to_string()))
                    .when(!hint.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(hint.to_string()),
                        )
                    }),
            )
            .child(
                Switch::new(id)
                    .checked(checked)
                    .accessibility_label(title)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_appearance(cx, |a| on_change(a, on));
                    })),
            )
            .into_any_element()
    }

    /// Slice chatlist-list-style: chat-list row style — preview line
    /// count (kit RadioGroup, like the theme/chat-style sections), media
    /// icons and formatted preview text (kit Switch rows).
    pub(in crate::ui) fn appearance_chat_list_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let lines = self.appearance.preview_lines;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-chat-list-lines")
            .selected_index(Some(if lines >= 3 { 1 } else { 0 }))
            .children([
                Radio::new("appearance-chat-list-lines-2").label(tr(keys::Q_PREFS_2_LINES)),
                Radio::new("appearance-chat-list-lines-3").label(tr(keys::Q_PREFS_3_LINES)),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| {
                    a.preview_lines = if ix == 0 { 2 } else { 3 };
                });
            }));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.appearance_section(
                cx,
                tr(keys::Q_PREFS_CHAT_LIST_ROWS),
                tr(keys::Q_PREFS_TWO_LINES_SHOWS_THE_TITLE),
                control.into_any_element(),
            ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-media-icons",
            tr(keys::Q_PREFS_MEDIA_ICONS),
            tr(keys::Q_PREFS_SHOW_A_PHOTO_VIDEO_OR),
            self.appearance.chat_list_media_icons,
            |a, on| a.chat_list_media_icons = on,
        ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-rich-preview",
            tr(keys::Q_PREFS_FORMATTED_PREVIEW_TEXT),
            tr(keys::Q_PREFS_SHOW_BOLD_ITALIC_AND_OTHER),
            self.appearance.chat_list_rich_preview,
            |a, on| a.chat_list_rich_preview = on,
        ));
        body = body.child(self.appearance_swipe_action_section(cx));
        body.into_any_element()
    }

    /// tdesktop's "Chat list quick action" (Settings > Chats): what a
    /// horizontal trackpad swipe on a chat row does. Disabled by default.
    pub(super) fn appearance_swipe_action_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::chat_swipe::SwipeAction;
        let current = self.appearance.swipe_action;
        let selected = SwipeAction::ALL.iter().position(|a| *a == current);
        let control = RadioGroup::vertical("appearance-swipe-action")
            .selected_index(selected)
            .children(SwipeAction::ALL.iter().map(|action| {
                Radio::new(("appearance-swipe-action", *action as usize))
                    .label(action.settings_label())
            }))
            .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                if let Some(action) = SwipeAction::ALL.get(ix).copied() {
                    this.set_appearance(cx, |a| a.swipe_action = action);
                }
            }));
        self.appearance_section(
            cx,
            tr(keys::SETTINGS_QUICK_DIALOG_ACTION_TITLE),
            tr(keys::Q_PREFS_MIDDLE_CLICK_A_CHAT_OR),
            control.into_any_element(),
        )
    }

    /// Send-key mode section (parity:settings-enter-send,
    /// parity:settings-ctrlenter-send): which keystroke sends a chat
    /// message. Lives in the Appearance dialog — Quill has no separate
    /// Chat Settings screen yet.
    /// `parity:platform-autostart` — "Launch at login" switch. OS-level
    /// (XDG Autostart on Linux, LaunchAgents on macOS); unsupported
    /// platforms render an explanatory line instead of the switch.
    pub(in crate::ui) fn general_autostart_section(&self, cx: &mut Context<Self>) -> AnyElement {
        if !quill::autostart::supported() {
            return self.appearance_section(
                cx,
                tr(keys::Q_PREFS_LAUNCH_AT_LOGIN),
                tr(keys::Q_PREFS_AUTOSTART_IS_NOT_AVAILABLE_ON),
                div().into_any_element(),
            );
        }
        let on = quill::autostart::is_enabled();
        let control = Switch::new("general-autostart-switch")
            .checked(on)
            .accessibility_label(tr(keys::Q_PREFS_LAUNCH_QUILL_AT_LOGIN))
            .on_click(cx.listener(|this, &on, _, cx| {
                if let Err(err) = quill::autostart::set_enabled(on) {
                    this.connection.set_note(StatusNote::error(err.to_string()));
                }
                cx.notify();
            }));
        self.appearance_toggle_section(
            cx,
            tr(keys::Q_PREFS_LAUNCH_AT_LOGIN),
            tr(keys::Q_PREFS_START_QUILL_AUTOMATICALLY_WHEN_YOU),
            control.into_any_element(),
        )
    }

    /// The handler is read on a thread (Linux): draw again when it lands.
    fn repaint_when_link_handler_read(&self, cx: &mut Context<Self>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static WATCHING: AtomicBool = AtomicBool::new(false);
        if !quill::link_handler::refreshing() || WATCHING.swap(true, Ordering::AcqRel) {
            return;
        }
        cx.spawn(async move |this, cx| {
            while quill::link_handler::refreshing() {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
            WATCHING.store(false, Ordering::Release);
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// "Open Telegram links with Quill": opt-in default handler for `tg:`
    /// links (`quill::link_handler`). Quill never claims it on its own.
    /// Windows/Linux get a switch; macOS cannot hand the scheme back to
    /// another app, so it gets a one-way "Make default" button.
    pub(in crate::ui) fn general_link_handler_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::link_handler::{self, ControlKind, LinkHandlerState};
        let state = link_handler::state();
        self.repaint_when_link_handler_read(cx);
        let ours = state == LinkHandlerState::Ours;
        // Nothing to turn on or off while the handler is unknown: it cannot be
        // managed, or the first read has not landed yet (drawn as checking).
        let unavailable = matches!(
            state,
            LinkHandlerState::Unavailable(_) | LinkHandlerState::Checking
        );
        let hint = if unavailable {
            link_handler::status_text(&state)
        } else {
            format!(
                "{} {}",
                link_handler::status_text(&state),
                link_handler::turn_off_note()
            )
        };
        let control: AnyElement = match link_handler::control_kind() {
            ControlKind::Button if ours => div()
                .text_sm()
                .child(tr(keys::Q_PREFS_QUILL_IS_THE_DEFAULT))
                .into_any_element(),
            ControlKind::Button => Button::new("general-link-handler-button")
                .label(tr(keys::Q_PREFS_MAKE_QUILL_THE_DEFAULT))
                .disabled(unavailable)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = link_handler::make_default() {
                        this.connection.set_note(StatusNote::error(err));
                    }
                    // The system confirmation is asynchronous: re-read the
                    // handler a few times so the status catches up.
                    for secs in [1u64, 3, 8] {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(secs))
                                .await;
                            link_handler::read_now();
                            let _ = this.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                    }
                    cx.notify();
                }))
                .into_any_element(),
            ControlKind::Switch => Switch::new("general-link-handler-switch")
                .checked(ours)
                .disabled(unavailable)
                .accessibility_label(tr(keys::Q_PREFS_OPEN_TELEGRAM_LINKS_WITH_QUILL))
                .on_click(cx.listener(|this, &on, _, cx| {
                    let result = if on {
                        link_handler::make_default()
                    } else {
                        link_handler::release()
                    };
                    if let Err(err) = result {
                        this.connection.set_note(StatusNote::error(err));
                    }
                    cx.notify();
                }))
                .into_any_element(),
        };
        self.appearance_toggle_section(
            cx,
            tr(keys::Q_PREFS_OPEN_TELEGRAM_LINKS_WITH_QUILL),
            &hint,
            control,
        )
    }

    pub(in crate::ui) fn appearance_send_key_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::composer::SendKeyMode;
        let current = self.chat_prefs.send_key_mode;
        let control = RadioGroup::horizontal("appearance-send-key")
            .selected_index(Some(if current == SendKeyMode::Enter { 0 } else { 1 }))
            .children([
                Radio::new("appearance-send-key-enter").label(tr(keys::Q_PREFS_ENTER)),
                Radio::new("appearance-send-key-ctrlenter").label(if cfg!(target_os = "macos") {
                    tr(keys::Q_PREFS_CMD_ENTER)
                } else {
                    tr(keys::Q_PREFS_CTRL_ENTER)
                }),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_chat_prefs(cx, |c| {
                    c.send_key_mode = if ix == 0 {
                        SendKeyMode::Enter
                    } else {
                        SendKeyMode::CtrlEnter
                    };
                });
            }));
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_SEND_MESSAGES_WITH),
            tr(keys::Q_PREFS_ENTER_SENDS_OR_ENTER_INSERTS),
            control.into_any_element(),
        )
    }

    /// parity:platform-spellcheck: the spellcheck toggle (same row
    /// pattern as `appearance_switch_row`, but wired to ChatPrefs), plus
    /// tdesktop's spell-checker language list where Quill owns the
    /// dictionaries (Hunspell on Linux, ISpellChecker languages on
    /// Windows; macOS lets the system pick).
    pub(in crate::ui) fn appearance_spellcheck_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self.chat_prefs.spellcheck_enabled;
        let hint = self.spell.info.hint();
        let row = div()
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
                    .child(div().text_sm().child(tr(keys::Q_PREFS_CHECK_SPELLING)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(hint),
                    ),
            )
            .child(
                Switch::new("appearance-spellcheck")
                    .checked(checked)
                    .accessibility_label(tr(keys::Q_PREFS_CHECK_SPELLING))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.spellcheck_enabled = on);
                        let text = this.composer.read(cx).value().to_string();
                        this.sync_spellcheck(&text, cx);
                    })),
            );
        let mut control = div().flex().flex_col().gap_2().child(row);
        if cfg!(target_os = "macos") {
            control = control.child(self.system_text_replace_row(cx));
        }
        if checked && !self.spell.info.available.is_empty() {
            let active = self.spell.info.active.clone();
            let mut chips = div().flex().flex_wrap().gap_2().child(self.appearance_chip(
                "spell-lang-auto",
                tr(keys::Q_PREFS_AUTOMATIC),
                self.spell.info.chosen.is_empty(),
                cx,
                |this, cx| this.set_spell_languages(Vec::new(), cx),
            ));
            for code in self.spell.info.available.clone() {
                let on = active.contains(&code);
                let base = active.clone();
                let toggled = code.clone();
                chips = chips.child(self.appearance_chip(
                    SharedString::from(format!("spell-lang-{code}")),
                    code.clone(),
                    on && !self.spell.info.chosen.is_empty(),
                    cx,
                    move |this, cx| {
                        let mut next = if this.spell.info.chosen.is_empty() {
                            base.clone()
                        } else {
                            this.spell.info.chosen.clone()
                        };
                        if let Some(i) = next.iter().position(|c| *c == toggled) {
                            next.remove(i);
                        } else {
                            next.push(toggled.clone());
                        }
                        this.set_spell_languages(next, cx);
                    },
                ));
            }
            control = control.child(chips);
        }
        if checked && self.dictionary_manager_available() {
            control = control.child(self.dictionary_manager_section(cx));
        }
        self.appearance_section(
            cx,
            tr(keys::Q_PREFS_SPELLING),
            "",
            control.into_any_element(),
        )
    }

    /// tdesktop "System text replacements" (`systemTextReplace`, macOS
    /// only): the user's Keyboard → Text Replacements apply in the composer.
    fn system_text_replace_row(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::SETTINGS_SYSTEM_TEXT_REPLACE)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_USE_THE_SHORTCUTS_FROM_KEYBOARD)),
                    ),
            )
            .child(
                Switch::new("appearance-system-text-replace")
                    .checked(self.chat_prefs.system_text_replace)
                    .accessibility_label(tr(keys::SETTINGS_SYSTEM_TEXT_REPLACE))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.system_text_replace = on);
                    })),
            )
            .into_any_element()
    }

    /// tdesktop "Suggest emoji replacements" (`suggestEmoji`, default on):
    /// the `:name` emoji popup in the composer.
    pub(in crate::ui) fn appearance_suggest_emoji_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let suggest = div()
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
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_SUGGEST_EMOJI_REPLACEMENTS)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_TYPE_AND_A_NAME_IN)),
                    ),
            )
            .child(
                Switch::new("appearance-suggest-emoji")
                    .checked(self.chat_prefs.suggest_emoji)
                    .accessibility_label(tr(keys::Q_PREFS_SUGGEST_EMOJI_REPLACEMENTS))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.suggest_emoji = on);
                        this.sync_suggest_menu(false, cx);
                    })),
            );
        // Quill's own: soft highlights on words that have an emoji
        // (`emoji_words`), part of the suggestions so it follows them.
        let highlight = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .pl_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_HIGHLIGHT_EMOJI_WORDS)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_HIGHLIGHT_EMOJI_WORDS_ABOUT)),
                    ),
            )
            .child(
                Switch::new("appearance-highlight-emoji-words")
                    .checked(self.chat_prefs.suggest_emoji && self.chat_prefs.highlight_emoji_words)
                    .disabled(!self.chat_prefs.suggest_emoji)
                    .accessibility_label(tr(keys::Q_PREFS_HIGHLIGHT_EMOJI_WORDS))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.highlight_emoji_words = on);
                    })),
            );
        // tdesktop "Replace emoji automatically" (`replaceEmoji`).
        let replace = div()
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
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_REPLACE_EMOJI_AUTOMATICALLY)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_TURN_AND_3_INTO_EMOJI)),
                    ),
            )
            .child(
                Switch::new("appearance-replace-emoji")
                    .checked(self.chat_prefs.replace_emoji)
                    .accessibility_label(tr(keys::Q_PREFS_REPLACE_EMOJI_AUTOMATICALLY))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.replace_emoji = on);
                    })),
            );
        let control = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(suggest)
            .child(highlight)
            .children(self.emoji_word_blocklist_row(cx))
            .child(replace)
            .child(self.emoji_set_settings_row(cx))
            .into_any_element();
        // tdesktop Chat settings "Large emoji": one to three emoji alone in
        // a message show as big glyphs (the history renderer reads the same
        // `MediaPrefs::big_emoji`).
        let large = div()
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
                    .child(div().text_sm().child(tr(keys::Q_PREFS_LARGE_EMOJI)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_SHOW_A_MESSAGE_OF_ONE)),
                    ),
            )
            .child(
                Switch::new("appearance-large-emoji")
                    .checked(
                        self.session()
                            .is_none_or(|s| s.settings.media_prefs.big_emoji),
                    )
                    .accessibility_label(tr(keys::Q_PREFS_LARGE_EMOJI))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_media_pref(|prefs| prefs.big_emoji = on, cx)
                    })),
            );
        let control = div().flex().flex_col().gap_2().child(large).child(control);
        self.appearance_section(cx, tr(keys::Q_PREFS_EMOJI), "", control.into_any_element())
    }
}

//! Methods moved out of `appearance.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// The custom accent field's state, showing `accent_rgb` (or the
    /// theme's accent when that is the default). A committed color is
    /// limited like tdesktop's (`limit_custom_accent`) and becomes the
    /// accent.
    pub(in crate::ui) fn new_accent_picker(
        accent_rgb: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        let initial = if accent_rgb == 0 {
            Hsla::from(super::super::chat_theme::accent_strong())
        } else {
            Hsla::from(rgb(accent_rgb))
        };
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial));
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
                this.set_appearance(cx, |a| {
                    a.accent_rgb = value;
                    a.system_accent = false;
                });
            },
        )
        .detach();
        picker
    }

    /// tdesktop's "Interface scale" (Settings > Chat settings). GPUI has no
    /// window-wide scale factor, so this scales the rem size: text and
    /// rem-based spacing follow, fixed-pixel widths (avatars, chat rows) do
    /// not — the hint says so.
    /// Telegram's wallpapers (`getInstalledBackgrounds`): color and gradient
    /// fills paint exactly, photos show once downloaded, patterns show their
    /// fill (the pattern layer, blur and motion are not drawn).
    pub(super) fn appearance_telegram_wallpapers_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark = cx.theme().is_dark();
        let session = self.session();
        let list = session.and_then(|s| s.chats_state.installed_backgrounds.clone());
        let default_id = session
            .and_then(|s| s.chats_state.default_backgrounds.get(&dark))
            .map(|b| b.id)
            .filter(|_| self.appearance.telegram_wallpaper);
        let error = session.and_then(|s| s.chats_state.background_error.clone());
        let muted = cx.theme().muted_foreground;
        let mut grid = div().flex().flex_wrap().gap_2();
        match &list {
            None => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading wallpapers…"),
                );
            }
            Some(list) if list.is_empty() => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("No wallpapers installed on this account."),
                );
            }
            Some(list) => {
                for background in list {
                    let id = background.id;
                    let selected = default_id == Some(id);
                    let mut tile = self
                        .wallpaper_tile(
                            ("appearance-tg-wallpaper", id as u64),
                            background,
                            selected,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_telegram_wallpaper(id, cx)
                        }));
                    tile = tile.child(
                        div().absolute().top_0().right_0().child(
                            Button::new(("appearance-tg-wallpaper-remove", id as u64))
                                .icon(gpui_kit::assets::IconName::X)
                                .ghost()
                                .xsmall()
                                .tooltip("Remove from installed wallpapers")
                                .accessibility_label("Remove wallpaper")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_telegram_wallpaper(id, cx);
                                })),
                        ),
                    );
                    grid = grid.child(tile);
                }
            }
        }
        let mut body = div().flex().flex_col().gap_2().child(grid).child(
            div().flex().child(
                Button::new("appearance-tg-wallpaper-file")
                    .label("From file…")
                    .small()
                    .tooltip("Use a JPEG, PNG or WebP image as your wallpaper")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_wallpaper_file(cx))),
            ),
        );
        if let Some(error) = error {
            body = body.child(
                div()
                    .id("appearance-tg-wallpaper-error")
                    .text_xs()
                    .text_color(super::super::danger_dark())
                    .child(format!("Couldn’t update wallpapers: {error}")),
            );
        }
        self.appearance_section(
            cx,
            "Telegram wallpapers",
            "Your installed wallpapers: colors, gradients, patterns and photos. Chat wallpapers and themes are set per chat from its info panel. Blur and motion are not drawn.",
            body.into_any_element(),
        )
    }

    /// Make an installed wallpaper the account's default for the current
    /// theme and show it.
    pub(super) fn choose_telegram_wallpaper(&mut self, background_id: i64, cx: &mut Context<Self>) {
        let dark = cx.theme().is_dark();
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.set_default_background(background_id, dark) {
                    self.connection.status_note = format!("could not set the wallpaper: {err:?}");
                    return;
                }
            }
            None => {
                // Screenshot demo: apply locally.
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(bg) = session
                        .chats_state
                        .installed_backgrounds
                        .iter()
                        .flatten()
                        .find(|b| b.id == background_id)
                        .cloned()
                {
                    session.chats_state.default_backgrounds.insert(dark, bg);
                }
            }
        }
        self.set_appearance(cx, |a| {
            a.telegram_wallpaper = true;
            a.wallpaper_rgb = None;
        });
    }

    pub(super) fn remove_telegram_wallpaper(&mut self, background_id: i64, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.remove_installed_background(background_id) {
                    self.connection.status_note =
                        format!("could not remove the wallpaper: {err:?}");
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(list) = session.chats_state.installed_backgrounds.as_mut()
                {
                    list.retain(|b| b.id != background_id);
                }
            }
        }
        cx.notify();
    }

    /// Fetch the installed wallpapers when Appearance opens, and start the
    /// photo downloads.
    pub(crate) fn ensure_wallpapers_loaded(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme().is_dark();
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .session
                .chats_state
                .installed_backgrounds
                .is_none()
            {
                let _ = live.driver.fetch_installed_backgrounds(dark);
            }
            live.driver.download_background_files();
        }
    }

    pub(super) fn appearance_scale_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
            "Interface scale",
            "Scales the whole interface, applied right away.",
            row.into_any_element(),
        )
    }

    pub(super) fn appearance_wallpaper_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
                "Default",
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
                name,
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
            "Chat wallpaper",
            "Solid color behind the message list — dark colors pair best with the dark theme.",
            row.into_any_element(),
        )
    }

    pub(super) fn appearance_font_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let size = self.appearance.font_size_px;
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("appearance-font-down")
                    .label("A−")
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
                    .label("A+")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.bump_font_size(cx, 1))),
            );
        self.appearance_section(
            cx,
            "Message text size",
            "Chat message text, 12–20 px.",
            control.into_any_element(),
        )
    }

    pub(super) fn appearance_bubble_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let bubbles = self.appearance.bubbles;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-style")
            .selected_index(Some(if bubbles { 0 } else { 1 }))
            .children([
                Radio::new("appearance-style-bubbles").label("💬 Bubbles"),
                Radio::new("appearance-style-plain").label("📄 Plain"),
            ])
            .on_click(cx.listener(|this, &ix, _, cx| {
                this.set_appearance(cx, |a| a.bubbles = ix == 0);
            }));
        self.appearance_section(
            cx,
            "Chat style",
            "Bubbles or plain rows without bubble backgrounds.",
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
    pub(super) fn appearance_chat_list_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let lines = self.appearance.preview_lines;
        // kit Phase 6 style: a kit RadioGroup (was: hand-rolled chips).
        let control = RadioGroup::horizontal("appearance-chat-list-lines")
            .selected_index(Some(if lines >= 3 { 1 } else { 0 }))
            .children([
                Radio::new("appearance-chat-list-lines-2").label("2 lines"),
                Radio::new("appearance-chat-list-lines-3").label("3 lines"),
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
            "Chat list rows",
            "Two lines shows the title plus the message preview; three lines adds the sender line.",
            control.into_any_element(),
        ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-media-icons",
            "Media icons",
            "Show a photo, video or file icon before the preview text.",
            self.appearance.chat_list_media_icons,
            |a, on| a.chat_list_media_icons = on,
        ));
        body = body.child(self.appearance_switch_row(
            cx,
            "appearance-chat-list-rich-preview",
            "Formatted preview text",
            "Show bold, italic and other formatting in the chat-list preview.",
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
            "Chat list quick action",
            "Middle-click a chat, or swipe it left with two fingers on a trackpad, \
             to run this action. A swipe runs it once you pass the threshold and \
             lift your fingers.",
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
    pub(super) fn general_autostart_section(&self, cx: &mut Context<Self>) -> AnyElement {
        if !quill::autostart::supported() {
            return self.appearance_section(
                cx,
                "Launch at login",
                "Autostart is not available on this platform.",
                div().into_any_element(),
            );
        }
        let on = quill::autostart::is_enabled();
        let control = Switch::new("general-autostart-switch")
            .checked(on)
            .accessibility_label("Launch Quill at login")
            .on_click(cx.listener(|this, &on, _, cx| {
                if let Err(err) = quill::autostart::set_enabled(on) {
                    this.connection.status_note = err.to_string();
                }
                cx.notify();
            }));
        self.appearance_section(
            cx,
            "Launch at login",
            "Start Quill automatically when you sign in to this device.",
            control.into_any_element(),
        )
    }

    /// "Open Telegram links with Quill": opt-in default handler for `tg:`
    /// links (`quill::link_handler`). Quill never claims it on its own.
    /// Windows/Linux get a switch; macOS cannot hand the scheme back to
    /// another app, so it gets a one-way "Make default" button.
    pub(super) fn general_link_handler_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::link_handler::{self, ControlKind, LinkHandlerState};
        let state = link_handler::state();
        let ours = state == LinkHandlerState::Ours;
        let unavailable = matches!(state, LinkHandlerState::Unavailable(_));
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
                .child("Quill is the default")
                .into_any_element(),
            ControlKind::Button => Button::new("general-link-handler-button")
                .label("Make Quill the default")
                .disabled(unavailable)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = link_handler::make_default() {
                        this.connection.status_note = err;
                    }
                    // The system confirmation is asynchronous: re-read the
                    // handler a few times so the status catches up.
                    for secs in [1u64, 3, 8] {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(secs))
                                .await;
                            link_handler::invalidate();
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
                .accessibility_label("Open Telegram links with Quill")
                .on_click(cx.listener(|this, &on, _, cx| {
                    let result = if on {
                        link_handler::make_default()
                    } else {
                        link_handler::release()
                    };
                    if let Err(err) = result {
                        this.connection.status_note = err;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        };
        self.appearance_section(cx, "Open Telegram links with Quill", &hint, control)
    }

    pub(super) fn appearance_send_key_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use quill::composer::SendKeyMode;
        let current = self.chat_prefs.send_key_mode;
        let control = RadioGroup::horizontal("appearance-send-key")
            .selected_index(Some(if current == SendKeyMode::Enter { 0 } else { 1 }))
            .children([
                Radio::new("appearance-send-key-enter").label("⏎ Enter"),
                Radio::new("appearance-send-key-ctrlenter").label(if cfg!(target_os = "macos") {
                    "⌘⏎ Cmd+Enter"
                } else {
                    "⌃⏎ Ctrl+Enter"
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
            "Send messages with",
            "Enter sends, or Enter inserts a newline and Ctrl/Cmd+Enter sends.",
            control.into_any_element(),
        )
    }

    /// parity:platform-spellcheck: the spellcheck toggle (same row
    /// pattern as `appearance_switch_row`, but wired to ChatPrefs), plus
    /// tdesktop's spell-checker language list where Quill owns the
    /// dictionaries (Hunspell on Linux, ISpellChecker languages on
    /// Windows; macOS lets the system pick).
    pub(super) fn appearance_spellcheck_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(div().text_sm().child("Check spelling"))
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
                    .accessibility_label("Check spelling")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.spellcheck_enabled = on);
                        let text = this.composer.read(cx).value().to_string();
                        this.sync_spellcheck(&text, cx);
                    })),
            );
        let mut control = div().flex().flex_col().gap_2().child(row);
        if checked && !self.spell.info.available.is_empty() {
            let active = self.spell.info.active.clone();
            let mut chips = div().flex().flex_wrap().gap_2().child(self.appearance_chip(
                "spell-lang-auto",
                "Automatic",
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
        self.appearance_section(cx, "Spelling", "", control.into_any_element())
    }

    /// tdesktop "Suggest emoji replacements" (`suggestEmoji`, default on):
    /// the `:name` emoji popup in the composer.
    pub(super) fn appearance_suggest_emoji_section(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(div().text_sm().child("Suggest emoji replacements"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Type : and a name in the composer to pick a matching emoji."),
                    ),
            )
            .child(
                Switch::new("appearance-suggest-emoji")
                    .checked(self.chat_prefs.suggest_emoji)
                    .accessibility_label("Suggest emoji replacements")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.suggest_emoji = on);
                        this.sync_suggest_menu(cx);
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
                    .child(div().text_sm().child("Replace emoji automatically"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Turn :-) and <3 into emoji as you type."),
                    ),
            )
            .child(
                Switch::new("appearance-replace-emoji")
                    .checked(self.chat_prefs.replace_emoji)
                    .accessibility_label("Replace emoji automatically")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_chat_prefs(cx, |c| c.replace_emoji = on);
                    })),
            );
        let control = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(suggest)
            .child(replace)
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
                    .child(div().text_sm().child("Large emoji"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show a message of one to three emoji as big glyphs."),
                    ),
            )
            .child(
                Switch::new("appearance-large-emoji")
                    .checked(
                        self.session()
                            .is_none_or(|s| s.settings.media_prefs.big_emoji),
                    )
                    .accessibility_label("Large emoji")
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_media_pref(|prefs| prefs.big_emoji = on, cx)
                    })),
            );
        let control = div().flex().flex_col().gap_2().child(large).child(control);
        self.appearance_section(cx, "Emoji", "", control.into_any_element())
    }

    /// Slice parity:settings-language: the app language picker (the IETF
    /// tag TDLib gets in `setTdlibParameters`; it was hardcoded "en"
    /// before this slice). TDLib reads parameters once at startup, so
    /// the chosen language applies after restart — no runtime
    /// application is attempted (see `settings::LanguagePrefs` for why
    /// `setOption("language_pack_id")` can't do it). The app's own
    /// strings stay English; this only changes the tag reported to
    /// Telegram.
    pub(super) fn appearance_language_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .session()
            .map(|s| s.settings.language_prefs.system_language_code.as_str())
            .unwrap_or(DEFAULT_LANGUAGE_CODE);
        let control = RadioGroup::vertical("appearance-language")
            .selected_index(
                SUPPORTED_LANGUAGES
                    .iter()
                    .position(|(code, _)| *code == current),
            )
            .children(SUPPORTED_LANGUAGES.iter().map(|(code, name)| {
                Radio::new(format!("appearance-language-{code}")).label(format!("{name} ({code})"))
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.set_language_pref(SUPPORTED_LANGUAGES[*ix].0, cx);
            }));
        self.appearance_section(
            cx,
            "Language",
            "The language reported to Telegram. Applies after restart — the app's own text stays English for now.",
            control.into_any_element(),
        )
    }
}

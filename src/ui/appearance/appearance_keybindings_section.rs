//! Methods moved out of `appearance.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Parity slice (platform-custom-keybindings): the shortcuts section.
    /// Each row shows the action and its current keystroke; "Change" arms
    /// keystroke capture for that row, and "Reset" restores the default.
    /// The chip shows only a chord that is actually bound. A press that
    /// collides with fixed chrome or another rebindable action is refused
    /// and explained under the row.
    pub(super) fn appearance_keybindings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::super::keybindings::{
            REBINDABLE_ACTIONS, apply_custom_bindings, conflict_message, resolve_keybindings,
        };
        use quill::settings::CustomKeybinding;

        let customs: Vec<CustomKeybinding> = self
            .live
            .as_ref()
            .map(|live| live.driver.load_custom_keybindings())
            .unwrap_or_default();
        let resolved = resolve_keybindings(&customs);
        let capturing = self.settings.keybinding_capture.clone();
        let rows = REBINDABLE_ACTIONS.iter().map(|ra| {
            let row_state = resolved.iter().find(|row| row.id == ra.id);
            let current = row_state
                .map(|row| row.live.join(" / "))
                .filter(|live| !live.is_empty())
                .unwrap_or_else(|| "—".to_string());
            let id = ra.id.to_string();
            let label = ra.label.to_string();
            let is_capturing = capturing.as_deref() == Some(ra.id);
            let persisted_error = row_state.and_then(|row| {
                let conflict = row.rejected.as_ref()?;
                let chord = customs
                    .iter()
                    .find(|custom| custom.id == ra.id)
                    .map(|custom| custom.keystroke.as_str())
                    .unwrap_or(ra.id);
                Some(conflict_message(chord, conflict))
            });
            let error = if is_capturing {
                None
            } else {
                self.settings
                    .keybinding_error
                    .as_ref()
                    .filter(|(err_id, _)| err_id == ra.id)
                    .map(|(_, message)| message.clone())
                    .or(persisted_error)
            };
            // A joined pair like "cmd-shift-g / ctrl-shift-g" is wider than
            // the row; stack those so the label does not paint under the chip.
            let key_lines: Vec<String> = if is_capturing {
                vec!["press keys…".to_string()]
            } else if current.chars().count() > 20 {
                current.split(" / ").map(str::to_string).collect()
            } else {
                vec![current]
            };
            let row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .py_1()
                .child(div().flex_1().min_w_0().text_sm().child(label))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_end()
                                .text_xs()
                                .font_family(super::super::message_text::MONO_FONT)
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(cx.theme().muted)
                                .children(key_lines.into_iter().map(|line| div().child(line))),
                        )
                        .child(
                            Button::new(format!("kb-change-{id}"))
                                .label(if is_capturing { "Cancel" } else { "Change" })
                                .small()
                                .ghost()
                                .on_click({
                                    let id = id.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        if this.settings.keybinding_capture.as_deref()
                                            == Some(id.as_str())
                                        {
                                            this.settings.keybinding_capture = None;
                                        } else {
                                            this.settings.keybinding_capture = Some(id.clone());
                                            window.focus(&this.settings.keybinding_focus, cx);
                                        }
                                        cx.notify();
                                    })
                                }),
                        )
                        .child(
                            Button::new(format!("kb-reset-{id}"))
                                .label("Reset")
                                .small()
                                .ghost()
                                .on_click({
                                    let id = id.clone();
                                    cx.listener(move |this, _, _, cx| {
                                        let custom = CustomKeybinding {
                                            id: id.clone(),
                                            keystroke: String::new(),
                                        };
                                        if let Some(live) = this.live.as_mut() {
                                            let _ = live.driver.save_custom_keybinding(custom);
                                            let customs = live.driver.load_custom_keybindings();
                                            apply_custom_bindings(cx, &customs);
                                        }
                                        this.settings.keybinding_capture = None;
                                        if this
                                            .settings
                                            .keybinding_error
                                            .as_ref()
                                            .is_some_and(|(err_id, _)| err_id == &id)
                                        {
                                            this.settings.keybinding_error = None;
                                        }
                                        cx.notify();
                                    })
                                }),
                        ),
                );
            let row = if is_capturing {
                row.track_focus(&self.settings.keybinding_focus)
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        // Stop the key before it dismisses Appearance or runs
                        // a global binding. GPUI also matches keybindings
                        // before this bubble handler; the capture interceptor
                        // consumes those. Modifier-only presses stay armed.
                        if this.keybinding_capture_active() {
                            cx.stop_propagation();
                            this.handle_keybinding_capture(&event.keystroke, cx);
                        }
                    }))
                    .into_any_element()
            } else {
                row.into_any_element()
            };
            if let Some(message) = error {
                div()
                    .flex()
                    .flex_col()
                    .child(row)
                    .child(
                        div()
                            .text_xs()
                            .text_color(super::super::danger())
                            .child(message),
                    )
                    .into_any_element()
            } else {
                row
            }
        });
        self.appearance_section(
            cx,
            "Keyboard shortcuts",
            "Rebind the shortcuts below. Changes apply immediately and are saved on this device. Window and app shortcuts (quit, close, …) can't be changed.",
            div().flex().flex_col().children(rows).into_any_element(),
        )
    }

    /// Apply one captured key while a shortcuts row is armed.
    ///
    /// Escape cancels. Bare modifiers are ignored so Ctrl+K can finish.
    /// A chord that collides with fixed chrome or another live shortcut is
    /// not saved; the active chip stays on the chord that is really bound.
    pub(in crate::ui) fn handle_keybinding_capture(
        &mut self,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) {
        use super::super::keybindings::{
            apply_custom_bindings, canonical_event_chord, conflict_message, is_modifier_key,
            keybinding_conflict,
        };
        use quill::settings::CustomKeybinding;

        if !self.keybinding_capture_active() {
            return;
        }
        let Some(id) = self.settings.keybinding_capture.clone() else {
            return;
        };
        if is_modifier_key(&keystroke.key) {
            return;
        }
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.settings.keybinding_capture = None;
            cx.notify();
            return;
        }
        let Some(chord) = canonical_event_chord(keystroke) else {
            return;
        };
        let customs = self
            .live
            .as_ref()
            .map(|live| live.driver.load_custom_keybindings())
            .unwrap_or_default();
        if let Some(conflict) = keybinding_conflict(&id, &chord, &customs) {
            self.settings.keybinding_error = Some((id, conflict_message(&chord, &conflict)));
            self.settings.keybinding_capture = None;
            cx.notify();
            return;
        }
        let custom = CustomKeybinding {
            id: id.clone(),
            keystroke: chord,
        };
        let saved = self.live.as_mut().map(|live| {
            let ok = live.driver.save_custom_keybinding(custom).is_ok();
            let customs = ok.then(|| live.driver.load_custom_keybindings());
            (ok, customs)
        });
        match saved {
            Some((true, Some(customs))) => {
                apply_custom_bindings(cx, &customs);
                if self
                    .settings
                    .keybinding_error
                    .as_ref()
                    .is_some_and(|(err_id, _)| err_id == &id)
                {
                    self.settings.keybinding_error = None;
                }
            }
            Some((false, _)) => {
                self.settings.keybinding_error = Some((
                    id,
                    "Couldn't save that shortcut. The previous one is still active.".into(),
                ));
            }
            _ => {}
        }
        self.settings.keybinding_capture = None;
        cx.notify();
    }
}

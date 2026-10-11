//! Methods moved out of `passcode.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(crate) fn build_passcode_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Passcode, |this, window, cx| {
                this.close_passcode(window, cx);
            });
        app.update(cx, |this, cx| {
            let body = this.passcode_body(cx);
            let title = match this.account.passcode.view {
                PasscodeView::Status => "Local passcode",
                PasscodeView::Create => "Create local passcode",
                PasscodeView::Change => "Change passcode",
                PasscodeView::Remove => "Turn off passcode",
            };
            let footer = div().flex().justify_end().child(
                Button::new("close-passcode")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_passcode(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::Passcode, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
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

    pub(super) fn passcode_field(
        &self,
        label: &'static str,
        input: &Entity<InputState>,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().font_semibold().child(label))
            .child(
                Input::new(input)
                    .aria_label(label)
                    .content_type(InputContentType::Password)
                    .h(px(40.)),
            )
    }

    pub(super) fn passcode_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let ui = &self.account.passcode;
        let busy = ui.busy;
        let mut body = div().flex().flex_col().gap_3().w_full();
        if let Some(error) = ui.error.clone() {
            body = body.child(
                div()
                    .id("passcode-error")
                    .role(Role::Status)
                    .text_sm()
                    .text_color(danger())
                    .child(error),
            );
        }
        match ui.view {
            PasscodeView::Status => {
                body = body
                    .child(div().text_sm().text_color(muted).child(
                        "When a local passcode is set, a lock icon appears at the top of your \
                         chat list. Click it to lock Quill.",
                    ))
                    .child(div().text_sm().text_color(muted).child(
                        "Note: if you forget your passcode, you'll need to log out of Quill \
                         and log in again.",
                    ));
                if !ui.enabled {
                    body = body.child(
                        Button::new("passcode-turn-on")
                            .label("Turn on passcode")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Create, window, cx)
                            })),
                    );
                } else {
                    body = body.child(self.passcode_enabled_section(cx));
                }
            }
            PasscodeView::Create => {
                body = body
                    .child(self.passcode_field("Passcode", &ui.new))
                    .child(self.passcode_field("Confirm passcode", &ui.confirm))
                    .child(div().text_xs().text_color(muted).child(
                        "The passcode protects your saved data on this device. It is never \
                         stored or sent anywhere.",
                    ))
                    .child(self.passcode_form_buttons(cx, "Save passcode", false));
            }
            PasscodeView::Change => {
                body = body
                    .child(self.passcode_field("Current passcode", &ui.old))
                    .child(self.passcode_field("New passcode", &ui.new))
                    .child(self.passcode_field("Confirm new passcode", &ui.confirm))
                    .child(self.passcode_form_buttons(cx, "Save passcode", false));
            }
            PasscodeView::Remove => {
                body = body
                    .child(div().text_sm().text_color(muted).child(
                        "Quill will open without asking for a passcode. Your saved data goes \
                         back to being protected by this device's keychain only.",
                    ))
                    .child(self.passcode_field("Current passcode", &ui.old))
                    .child(self.passcode_form_buttons(cx, "Turn off passcode", true));
            }
        }
        let _ = busy;
        body
    }

    pub(super) fn passcode_form_buttons(
        &self,
        cx: &mut Context<Self>,
        label: &'static str,
        danger_action: bool,
    ) -> Div {
        let busy = self.account.passcode.busy;
        let mut submit = Button::new("passcode-submit")
            .label(if busy { "Working…" } else { label })
            .loading(busy)
            .disabled(busy)
            .on_click(cx.listener(|this, _, window, cx| this.submit_passcode_form(window, cx)));
        submit = if danger_action {
            submit.danger()
        } else {
            submit.primary()
        };
        div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
                Button::new("passcode-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.goto_passcode_view(PasscodeView::Status, window, cx)
                    })),
            )
            .child(submit)
    }

    pub(super) fn passcode_enabled_section(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let secs = self.account.passcode.autolock_secs;
        let mut section =
            div().flex().flex_col().gap_3().child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("passcode-change")
                            .label("Change passcode")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Change, window, cx)
                            })),
                    )
                    .child(Button::new("passcode-lock-now").label("Lock now").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.close_passcode(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::Passcode, window, cx);
                            this.lock_by_passcode(cx);
                        }),
                    ))
                    .child(
                        Button::new("passcode-remove")
                            .label("Turn off passcode")
                            .ghost()
                            .custom(super::security::quiet_danger(cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.goto_passcode_view(PasscodeView::Remove, window, cx)
                            })),
                    ),
            );
        let mut presets = div().flex().flex_wrap().gap_1();
        for preset in AUTOLOCK_PRESETS {
            presets = presets.child(
                Button::new(SharedString::from(format!("autolock-{preset}")))
                    .label(passcode::autolock_label(preset))
                    .outline()
                    .selected(preset == secs)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.account.passcode.error = None;
                        this.set_autolock_secs(preset, cx)
                    })),
            );
        }
        let custom = !AUTOLOCK_PRESETS.contains(&secs);
        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(format!(
                    "{} {}",
                    PasscodeUi::autolock_title(),
                    passcode::autolock_label(secs)
                )))
                .child(presets)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().w(px(96.)).child(
                                Input::new(&self.account.passcode.custom_time)
                                    .aria_label("Custom auto-lock time, hours and minutes")
                                    .h(px(32.)),
                            ),
                        )
                        .child(
                            Button::new("autolock-custom")
                                .label("Set custom time")
                                .outline()
                                .selected(custom)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.apply_custom_autolock(cx)),
                                ),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Custom time is hours:minutes, like 0:30."),
                ),
        );
        if self.system_unlock_offered() {
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(super::system_unlock::label()),
                            )
                            .child(div().text_xs().text_color(muted).child(
                                "Enter your passcode once after each launch, then unlock with \
                                 the system prompt.",
                            )),
                    )
                    .child(
                        Switch::new("passcode-system-unlock")
                            .checked(self.account.passcode.system_unlock)
                            .accessibility_label(super::system_unlock::label())
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_system_unlock(on, cx);
                            })),
                    ),
            );
        }
        section
    }
}

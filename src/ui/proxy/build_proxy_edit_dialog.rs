//! Methods moved out of `proxy.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn build_proxy_edit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyEdit, |this, _, cx| {
                this.close_proxy_editor(cx);
            });
        app.update(cx, |this, cx| {
            let Some(editor) = this.settings.proxy.editor.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Edit proxy"))
                    .on_close(on_close);
            };
            let editing = editor.id;
            let kind = editor.kind;
            let error = editor.error.clone();
            let (server, port, username, password, secret) = (
                editor.server.clone(),
                editor.port.clone(),
                editor.username.clone(),
                editor.password.clone(),
                editor.secret.clone(),
            );
            let kinds = [ProxyKind::Socks5, ProxyKind::Http, ProxyKind::Mtproto];
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    RadioGroup::horizontal("proxy-kind")
                        .selected_index(kinds.iter().position(|k| *k == kind))
                        .children(kinds.iter().map(|k| {
                            Radio::new(format!("proxy-kind-{}", kind_label(*k)))
                                .label(kind_label(*k))
                        }))
                        .on_click(cx.listener(move |this, &ix, _, cx| {
                            if let Some(editor) = this.settings.proxy.editor.as_mut() {
                                editor.kind = kinds[ix];
                                editor.error = None;
                            }
                            cx.notify();
                        })),
                )
                .child(Self::editor_field(
                    cx,
                    "Hostname",
                    Input::new(&server).aria_label("Hostname").h(px(40.)),
                ))
                .child(Self::editor_field(
                    cx,
                    "Port",
                    Input::new(&port).aria_label("Port").h(px(40.)),
                ));
            if kind == ProxyKind::Mtproto {
                body = body.child(Self::editor_field(
                    cx,
                    "Secret",
                    Input::new(&secret).aria_label("Secret").h(px(40.)),
                ));
            } else {
                body = body
                    .child(Self::editor_field(
                        cx,
                        "Username (optional)",
                        Input::new(&username).aria_label("Username").h(px(40.)),
                    ))
                    .child(Self::editor_field(
                        cx,
                        "Password (optional)",
                        Input::new(&password)
                            .aria_label("Password")
                            .content_type(InputContentType::Password)
                            .h(px(40.)),
                    ));
            }
            if let Some(error) = error {
                body = body.child(div().text_xs().text_color(danger()).child(error));
            }
            let mut footer = div().flex().items_center().gap_2();
            if let Some(id) = editing {
                footer = footer
                    .child(
                        Button::new("proxy-edit-delete")
                            .label("Delete")
                            .danger()
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_proxy_entry(id, cx);
                                this.close_proxy_editor(cx);
                                this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-edit-copy")
                            .label("Copy link")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_proxy_link(id, cx);
                            })),
                    );
            }
            footer = footer
                .child(div().flex_1())
                .child(
                    Button::new("proxy-edit-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-edit-save")
                        .label("Save")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if editing.is_some() {
                    "Edit proxy"
                } else {
                    "Add proxy"
                }))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    pub(super) fn link_row(label: &'static str, value: String, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_start()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .font_semibold()
                    .text_right()
                    .child(value),
            )
    }

    pub(in crate::ui) fn build_proxy_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProxyLink, |this, _, cx| {
                this.close_proxy_link(cx);
            });
        app.update(cx, |this, cx| {
            let Some(link) = this.settings.proxy.link.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Proxy Server"))
                    .on_close(on_close);
            };
            let draft = link.draft.clone();
            let warn = link.warn;
            let ping = this
                .session()
                .and_then(|s| s.settings.proxy.pings.get(&LINK_PING_ID).copied());
            let mut body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Self::link_row("Server", draft.server.clone(), cx))
                .child(Self::link_row("Port", draft.port.to_string(), cx));
            if draft.kind == ProxyKind::Mtproto {
                body = body.child(Self::link_row("Secret", draft.secret.clone(), cx));
            } else {
                if !draft.username.is_empty() {
                    body = body.child(Self::link_row("Username", draft.username.clone(), cx));
                }
                if !draft.password.is_empty() {
                    body = body.child(Self::link_row("Password", draft.password.clone(), cx));
                }
            }
            let status = match ping {
                Some(status) => {
                    let (text, color) = match status {
                        PingStatus::Checking => ("Checking…".to_string(), text_muted()),
                        PingStatus::Available(ms) => {
                            (format!("Available (ping: {ms} ms)"), success())
                        }
                        PingStatus::Unavailable => ("Not Available".to_string(), danger()),
                    };
                    div().text_sm().font_semibold().text_color(color).child(text)
                }
                None => div().child(
                    Button::new("proxy-link-check")
                        .label("Check Status")
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.check_link_proxy(cx))),
                ),
            };
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Status"),
                    )
                    .child(status),
            );
            if warn {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_xs().text_color(text_muted()).child(
                            "This will expose your IP address to the admin of the proxy server.",
                        ))
                        .child(
                            Button::new("proxy-link-proceed")
                                .label("Proceed")
                                .xsmall()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_link_ip_warning(cx);
                                })),
                        ),
                );
            }
            if draft.kind == ProxyKind::Mtproto {
                body = body.child(div().text_xs().text_color(text_muted()).child(
                    "This proxy may display a sponsored channel in your chat list. This doesn't reveal any of your Telegram traffic.",
                ));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("proxy-link-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_link(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-link-connect")
                        .label("Connect Proxy")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.connect_link_proxy(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Proxy Server"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

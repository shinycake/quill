//! Methods moved out of `proxy.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

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
                    .title(crate::ui::shell::dialog_title(tr(keys::PROXY_EDIT)))
                    .on_close(on_close);
            };
            let editing = editor.id;
            let kind = editor.kind;
            let error = editor.error.clone();
            let (server, web_host, port, username, password, secret) = (
                editor.server.clone(),
                editor.web_host.clone(),
                editor.port.clone(),
                editor.username.clone(),
                editor.password.clone(),
                editor.secret.clone(),
            );
            let kinds = [
                ProxyKind::Socks5,
                ProxyKind::Http,
                ProxyKind::Mtproto,
                ProxyKind::Web,
            ];
            let mut body = div().flex().flex_col().gap_2().child(
                RadioGroup::horizontal("proxy-kind")
                    .selected_index(kinds.iter().position(|k| *k == kind))
                    .children(kinds.iter().map(|k| {
                        Radio::new(format!("proxy-kind-{}", kind_label(*k))).label(kind_label(*k))
                    }))
                    .on_click(cx.listener(move |this, &ix, _, cx| {
                        if let Some(editor) = this.settings.proxy.editor.as_mut()
                            && let Some(&kind) = kinds.get(ix)
                        {
                            editor.kind = kind;
                            editor.error = None;
                        }
                        cx.notify();
                    })),
            );
            // tdesktop shows the sponsor warning for MTProto and the web
            // warning for WEB right under the type radios.
            if matches!(kind, ProxyKind::Mtproto | ProxyKind::Web) {
                body = body.child(div().text_xs().text_color(text_muted()).child(
                    if kind == ProxyKind::Web {
                        tr(keys::PROXY_WEB_WARNING)
                    } else {
                        tr(keys::PROXY_SPONSOR_WARNING)
                    },
                ));
            }
            if kind == ProxyKind::Web {
                body = body.child(Self::editor_field(
                    cx,
                    tr(keys::PROXY_WEB_SERVER_LABEL),
                    Input::new(&web_host)
                        .aria_label(tr(keys::PROXY_WEB_SERVER_LABEL))
                        .h(px(40.)),
                ));
            } else {
                body = body
                    .child(Self::editor_field(
                        cx,
                        tr(keys::Q_PREFS_HOSTNAME),
                        Input::new(&server)
                            .aria_label(tr(keys::Q_PREFS_HOSTNAME))
                            .h(px(40.)),
                    ))
                    .child(Self::editor_field(
                        cx,
                        tr(keys::PROXY_BOX_PORT),
                        Input::new(&port)
                            .aria_label(tr(keys::PROXY_BOX_PORT))
                            .h(px(40.)),
                    ));
            }
            if matches!(kind, ProxyKind::Mtproto | ProxyKind::Web) {
                body = body.child(Self::editor_field(
                    cx,
                    tr(keys::PROXY_BOX_SECRET),
                    Input::new(&secret)
                        .aria_label(tr(keys::PROXY_BOX_SECRET))
                        .h(px(40.)),
                ));
            } else {
                body = body
                    .child(Self::editor_field(
                        cx,
                        tr(keys::Q_PREFS_USERNAME_OPTIONAL),
                        Input::new(&username)
                            .aria_label(tr(keys::PROXY_BOX_USERNAME))
                            .h(px(40.)),
                    ))
                    .child(Self::editor_field(
                        cx,
                        tr(keys::Q_PREFS_PASSWORD_OPTIONAL),
                        Input::new(&password)
                            .aria_label(tr(keys::PROXY_BOX_PASSWORD))
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
                            .label(tr(keys::SETTINGS_QUICK_DIALOG_ACTION_DELETE))
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
                            .label(tr(keys::CONTEXT_COPY_LINK))
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
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-edit-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_proxy_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyEdit, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if editing.is_some() {
                    tr(keys::PROXY_EDIT)
                } else {
                    tr(keys::PROXY_ADD)
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
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::PROXY_BOX_TABLE_TITLE,
                    )))
                    .on_close(on_close);
            };
            let draft = link.draft.clone();
            let warn = link.warn;
            let ping = this
                .session()
                .and_then(|s| s.settings.proxy.pings.get(&LINK_PING_ID).copied());
            let mut body = div().flex().flex_col().gap_2().child(Self::link_row(
                tr(keys::PROXY_BOX_SERVER),
                draft.display_address(),
                cx,
            ));
            // A web proxy has no port to show (always 443).
            if draft.kind != ProxyKind::Web {
                body = body.child(Self::link_row(
                    tr(keys::PROXY_BOX_PORT),
                    draft.port.to_string(),
                    cx,
                ));
            }
            if matches!(draft.kind, ProxyKind::Mtproto | ProxyKind::Web) {
                body = body.child(Self::link_row(
                    tr(keys::PROXY_BOX_SECRET),
                    draft.secret.clone(),
                    cx,
                ));
            } else {
                if !draft.username.is_empty() {
                    body = body.child(Self::link_row(
                        tr(keys::PROXY_BOX_USERNAME),
                        draft.username.clone(),
                        cx,
                    ));
                }
                if !draft.password.is_empty() {
                    body = body.child(Self::link_row(
                        tr(keys::PROXY_BOX_PASSWORD),
                        draft.password.clone(),
                        cx,
                    ));
                }
            }
            let status = match ping {
                Some(status) => {
                    let (text, color) = match status {
                        PingStatus::Checking => {
                            (tr(keys::PROXY_BOX_TABLE_CHECKING).to_string(), text_muted())
                        }
                        PingStatus::Available(ms) => (
                            tr_with(
                                keys::PROXY_BOX_TABLE_AVAILABLE,
                                &[("ping", &ms.to_string())],
                            ),
                            success(),
                        ),
                        PingStatus::Unavailable => {
                            (tr(keys::PROXY_BOX_TABLE_UNAVAILABLE).to_string(), danger())
                        }
                    };
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(color)
                        .child(text)
                }
                None => div().child(
                    Button::new("proxy-link-check")
                        .label(tr(keys::PROXY_BOX_CHECK_STATUS))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.check_link_proxy(cx))),
                ),
            };
            // tdesktop does not check the status of a web proxy.
            if draft.kind != ProxyKind::Web {
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
                                .child(tr(keys::PROXY_BOX_STATUS)),
                        )
                        .child(status),
                );
            }
            if warn {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child(tr(keys::PROXY_CHECK_IP_WARNING)),
                        )
                        .child(
                            Button::new("proxy-link-proceed")
                                .label(tr(keys::PROXY_CHECK_IP_PROCEED))
                                .xsmall()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_link_ip_warning(cx);
                                })),
                        ),
                );
            }
            if matches!(draft.kind, ProxyKind::Mtproto | ProxyKind::Web) {
                body = body.child(div().text_xs().text_color(text_muted()).child(
                    if draft.kind == ProxyKind::Web {
                        tr(keys::PROXY_WEB_WARNING)
                    } else {
                        tr(keys::PROXY_SPONSOR_WARNING)
                    },
                ));
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("proxy-link-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_proxy_link(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                )
                .child(
                    Button::new("proxy-link-connect")
                        .label(if draft.kind == ProxyKind::Web {
                            tr(keys::PROXY_ADD)
                        } else {
                            tr(keys::PROXY_BOX_TABLE_BUTTON)
                        })
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.connect_link_proxy(cx);
                            this.close_kit_dialog_if_done(DialogKind::ProxyLink, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::PROXY_BOX_TABLE_TITLE,
                )))
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

//! Connected websites dialog, moved out of `mod.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// kit Phase 2 (redo): connected websites hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_websites_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Websites, |this, _, cx| {
                this.close_websites(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let has_websites = session
                .as_ref()
                .is_some_and(|s| s.settings.connected_websites.data().is_some());
            let mut websites = session
                .as_ref()
                .and_then(|s| s.settings.connected_websites.data().cloned())
                .unwrap_or_default();
            websites.sort_by_key(|a| std::cmp::Reverse(a.last_active_date));
            let loading = session.is_some_and(|s| s.settings.connected_websites.is_loading());
            let mutating = session.is_some_and(|s| s.settings.websites_op.is_running());
            let stale = session.is_some_and(|s| s.settings.websites_stale);
            let error = session
                .as_ref()
                .and_then(|s| s.settings.connected_websites.error().map(str::to_string));

            let mut body = div().flex().flex_col().gap_2();
            if let Some(line) = error {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(danger())
                        .child(tr_with(keys::Q_PREFS_ERROR, &[("line", &line.to_string())])),
                );
            }
            if let Some(confirm) = this.privacy.websites.confirm() {
                body = body.child(this.websites_confirm_banner(confirm, &websites, mutating, cx));
            }
            if websites.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            tr(keys::Q_PREFS_LOADING)
                        } else if has_websites {
                            tr(keys::Q_PREFS_NO_ACTIVE_LOGINS_YOU_CAN)
                        } else {
                            tr(keys::Q_PREFS_NO_WEBSITE_DATA_YET)
                        }),
                );
            } else {
                if stale && loading {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_REFRESHING)),
                    );
                }
                body = body
                    .child(
                        div().flex().justify_end().child(
                            Button::new("disconnect-all-websites")
                                .label(tr(keys::SETTINGS_DISCONNECT_ALL))
                                .danger()
                                .disabled(mutating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_disconnect_all_websites(cx);
                                    this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                                })),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::SETTINGS_LOGGED_IN_DESCRIPTION)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::SETTINGS_LOGGED_IN)),
                    );
                // Phase 6: a one-column kit Table hosting the rich
                // website rows (was: bare rows).
                let mut websites_body = TableBody::new();
                for w in &websites {
                    websites_body = websites_body.child(
                        TableRow::new().child(Self::table_cell(this.website_row(w, mutating, cx))),
                    );
                }
                body = body.child(
                    Table::new()
                        .with_ix(3)
                        .accessibility_label(tr(keys::SETTINGS_LOGGED_IN))
                        .w_full()
                        .child(websites_body),
                );
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_PREFS_TAP_TO_DISCONNECT_FROM_YOUR)),
                );
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("websites-refresh")
                        .label(tr(keys::Q_PREFS_REFRESH))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_websites(cx);
                            this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                        })),
                )
                .child(
                    Button::new("close-websites")
                        .label(tr(keys::CLOSE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_websites(cx);
                            this.close_kit_dialog_if_done(DialogKind::Websites, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                // TGX `WebSessionsTitle`, verbatim.
                .title(crate::ui::shell::dialog_title(tr(
                    keys::SETTINGS_LOGGED_IN_TITLE,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
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
}

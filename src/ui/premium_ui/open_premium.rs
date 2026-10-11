//! Methods moved out of `premium_ui.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn open_premium(&mut self, cx: &mut Context<Self>) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.hub.premium_open = true;
            live.driver.maybe_fetch_premium()
        });
        if let Some(Err(_)) = sent {
            self.connection.status_note = "could not load Premium".into();
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_premium(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.premium_open = false;
        }
        cx.notify();
    }

    pub(in crate::ui) fn build_premium_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PremiumFeatures, |this, _, cx| {
                this.close_premium(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Telegram Premium"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let hub = &session.payments.hub;
            let muted = cx.theme().muted_foreground;
            let subscribed =
                hub.premium_state.as_ref().is_some_and(|s| s.is_subscribed) || session.is_premium();
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .py_2()
                    .child(div().text_size(px(34.)).child("\u{2B50}"))
                    .child(div().text_lg().font_semibold().child(if subscribed {
                        "You have Telegram Premium"
                    } else {
                        "Telegram Premium"
                    }))
                    .child(
                        div().text_sm().text_color(muted).text_center().child(
                            hub.premium_state
                                .as_ref()
                                .map(|s| s.text.clone())
                                .filter(|t| !t.is_empty())
                                .unwrap_or_else(|| "Extra features and higher limits.".to_string()),
                        ),
                    ),
            );
            if !subscribed {
                body = body.child(
                    Button::new("premium-get")
                        .label("Get Premium")
                        .primary()
                        .on_click(cx.listener(|_, _, _, cx| cx.open_url(PREMIUM_BOT_URL))),
                );
                body = body.child(div().text_xs().text_color(muted).text_center().child(
                    "Subscriptions are managed through @PremiumBot. Quill never takes payment.",
                ));
            }
            if let Some(err) = hub.premium_error.as_ref() {
                body = body.child(div().text_sm().child(err.clone()));
            }
            if hub.premium_loading && hub.premium.is_none() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Loading features\u{2026}"),
                );
            }
            if let Some(info) = hub.premium.as_ref() {
                for feature in &info.features {
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child(feature.title))
                            .child(div().text_xs().text_color(muted).child(feature.about)),
                    );
                }
                if !info.limits.is_empty() {
                    body = body.child(div().pt_2().font_semibold().child("Limits"));
                    for limit in &info.limits {
                        body = body.child(fact_row(
                            limit.title,
                            format!("{} \u{2192} {}", limit.default_value, limit.premium_value),
                            cx,
                        ));
                    }
                }
            }
            dialog
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
                .on_close(on_close)
        })
    }
}

//! "Show Your Last Seen" or Premium, the box the "when?" button opens
//! (tdesktop `Ui::ShowOrPremiumBox` with `ShowOrPremium::LastSeen`).
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use super::super::status_note::StatusNote;
use super::bold_marked;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::privacy::PrivacyRuleDetail;
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    fn close_last_seen_box(&mut self, cx: &mut Context<Self>) {
        self.profile_cover.last_seen_box = None;
        cx.notify();
    }

    /// "Show My Last Seen": last seen goes to Everybody, with no
    /// exceptions, as tdesktop saves it.
    fn show_my_last_seen(&mut self, cx: &mut Context<Self>) {
        let everybody = PrivacyRuleDetail {
            who: Some(PrivacyWho::Everybody),
            ..Default::default()
        };
        let sent = match self.live.as_mut() {
            Some(live) => live
                .driver
                .set_privacy_rules(PrivacySettingKey::ShowStatus, everybody)
                .is_ok(),
            None => true,
        };
        self.connection.set_note(if sent {
            StatusNote::info(tr(keys::LASTSEEN_SHOWN_TOAST))
        } else {
            StatusNote::error(tr(keys::Q_NOTES_FAILED_PRIVACY_UPDATE))
        });
        self.close_last_seen_box(cx);
    }

    pub(in crate::ui) fn build_last_seen_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ShowLastSeen, |this, _, cx| {
                this.close_last_seen_box(cx)
            });
        let name = {
            let app = app.read(cx);
            app.profile_cover
                .last_seen_box
                .and_then(|user_id| app.session()?.user(user_id))
                .map(|user| super::unmarked(&user.first_name))
                .unwrap_or_default()
        };
        app.update(cx, |_, cx| {
            let theme = cx.theme();
            let (muted, border, accent) = (theme.muted_foreground, theme.border, theme.primary);
            let title = |key| div().text_center().text_lg().font_semibold().child(tr(key));
            let about = |key| {
                div()
                    .text_center()
                    .text_sm()
                    .text_color(muted)
                    .child(bold_marked(&tr_with(key, &[("user", &name)])))
            };
            let line = || div().flex_1().h(px(1.)).bg(border);
            let body = div()
                .id("last-seen-box")
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(72.))
                        .rounded_full()
                        .bg(accent)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::Eye)
                                .size(px(36.))
                                .text_color(cx.theme().primary_foreground),
                        ),
                )
                .child(title(keys::LASTSEEN_SHOW_TITLE))
                .child(about(keys::LASTSEEN_SHOW_ABOUT))
                .child(
                    div()
                        .w_full()
                        .debug_selector(|| "last-seen-show".to_string())
                        .child(
                            Button::new("last-seen-show")
                                .label(tr(keys::LASTSEEN_SHOW_BUTTON))
                                .primary()
                                .w_full()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_my_last_seen(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::ShowLastSeen,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_sm()
                        .text_color(muted)
                        .child(line())
                        .child(tr(keys::LASTSEEN_OR))
                        .child(line()),
                )
                .child(title(keys::LASTSEEN_PREMIUM_TITLE))
                .child(about(keys::LASTSEEN_PREMIUM_ABOUT))
                .into_any_element();
            let footer = div().flex().w_full().child(
                div()
                    .flex_1()
                    .debug_selector(|| "last-seen-premium".to_string())
                    .child(
                        Button::new("last-seen-premium")
                            .label(tr(keys::LASTSEEN_PREMIUM_BUTTON))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_last_seen_box(cx);
                                this.close_kit_dialog_if_done(DialogKind::ShowLastSeen, window, cx);
                                this.open_premium_from("lastseen_hidden", cx);
                            })),
                    ),
            );
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

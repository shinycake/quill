//! "Show Your Last Seen" / "Show Your Read Date" or Premium (tdesktop
//! `Ui::ShowOrPremiumBox`). The profile's "when?" button opens the last
//! seen box; the message menu's "show when" opens the read time box.
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
use quill::lang::{Key, keys, tr, tr_with};
use quill::privacy::PrivacyRuleDetail;
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::cell::RefCell;
use std::rc::Rc;

/// What the box asks you to show (tdesktop `Ui::ShowOrPremium`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum ShowOrPremium {
    LastSeen,
    ReadTime,
}

/// The open box and the person it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct ShowOrPremiumBox {
    pub(in crate::ui) kind: ShowOrPremium,
    pub(in crate::ui) user_id: i64,
}

/// The words, icon and Premium ref of one kind (tdesktop's `Skin`).
struct Skin {
    show_title: Key,
    show_about: Key,
    show_button: Key,
    or: Key,
    premium_title: Key,
    premium_about: Key,
    premium_button: Key,
    icon: IconName,
    premium_ref: &'static str,
}

impl ShowOrPremium {
    fn skin(self) -> Skin {
        match self {
            Self::LastSeen => Skin {
                show_title: keys::LASTSEEN_SHOW_TITLE,
                show_about: keys::LASTSEEN_SHOW_ABOUT,
                show_button: keys::LASTSEEN_SHOW_BUTTON,
                or: keys::LASTSEEN_OR,
                premium_title: keys::LASTSEEN_PREMIUM_TITLE,
                premium_about: keys::LASTSEEN_PREMIUM_ABOUT,
                premium_button: keys::LASTSEEN_PREMIUM_BUTTON,
                icon: IconName::Eye,
                premium_ref: "lastseen_hidden",
            },
            Self::ReadTime => Skin {
                show_title: keys::READTIME_SHOW_TITLE,
                show_about: keys::READTIME_SHOW_ABOUT,
                show_button: keys::READTIME_SHOW_BUTTON,
                or: keys::READTIME_OR,
                premium_title: keys::READTIME_PREMIUM_TITLE,
                premium_about: keys::READTIME_PREMIUM_ABOUT,
                premium_button: keys::READTIME_PREMIUM_BUTTON,
                icon: IconName::CheckCheck,
                premium_ref: "revtime_hidden",
            },
        }
    }
}

impl QuillApp {
    /// Open the box for `user_id`.
    pub(in crate::ui) fn open_show_or_premium(
        &mut self,
        kind: ShowOrPremium,
        user_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.profile_cover.show_or_premium = Some(ShowOrPremiumBox { kind, user_id });
        cx.notify();
    }

    fn close_show_or_premium(&mut self, cx: &mut Context<Self>) {
        self.profile_cover.show_or_premium = None;
        cx.notify();
    }

    /// "Show My Last Seen": last seen goes to Everybody, with no
    /// exceptions, as tdesktop saves it. "Show My Read Time": read dates
    /// are shown again (`updateHideReadTime(false)`).
    fn show_mine(&mut self, kind: ShowOrPremium, cx: &mut Context<Self>) {
        let sent = match (kind, self.live.as_mut()) {
            (ShowOrPremium::LastSeen, Some(live)) => {
                let everybody = PrivacyRuleDetail {
                    who: Some(PrivacyWho::Everybody),
                    ..Default::default()
                };
                live.driver
                    .set_privacy_rules(PrivacySettingKey::ShowStatus, everybody)
                    .is_ok()
            }
            (ShowOrPremium::ReadTime, Some(live)) => {
                live.driver.set_read_date_privacy(true).is_ok()
            }
            (ShowOrPremium::ReadTime, None) => {
                if let Some(demo) = self.demo_session.as_mut() {
                    demo.settings.read_date_show = Some(true);
                }
                true
            }
            (ShowOrPremium::LastSeen, None) => true,
        };
        self.connection.set_note(match (sent, kind) {
            (true, ShowOrPremium::LastSeen) => StatusNote::info(tr(keys::LASTSEEN_SHOWN_TOAST)),
            (true, ShowOrPremium::ReadTime) => StatusNote::info(tr(keys::READTIME_SHOWN_TOAST)),
            (false, ShowOrPremium::LastSeen) => {
                StatusNote::error(tr(keys::Q_NOTES_FAILED_PRIVACY_UPDATE))
            }
            (false, ShowOrPremium::ReadTime) => {
                StatusNote::error(tr(keys::Q_NOTES_FAILED_READ_DATE))
            }
        });
        self.close_show_or_premium(cx);
    }

    pub(in crate::ui) fn build_show_or_premium_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ShowOrPremium, |this, _, cx| {
                this.close_show_or_premium(cx)
            });
        let (open, name) = {
            let app = app.read(cx);
            let open = app.profile_cover.show_or_premium;
            let name = open
                .and_then(|open| app.session()?.user(open.user_id))
                .map(|user| super::unmarked(&user.first_name))
                .unwrap_or_default();
            (open, name)
        };
        let kind = open.map_or(ShowOrPremium::LastSeen, |open| open.kind);
        let skin = kind.skin();
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
                .id("show-or-premium-box")
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
                            Icon::new(skin.icon)
                                .size(px(36.))
                                .text_color(cx.theme().primary_foreground),
                        ),
                )
                .child(title(skin.show_title))
                .child(about(skin.show_about))
                .child(
                    div()
                        .w_full()
                        .debug_selector(|| "show-or-premium-show".to_string())
                        .child(
                            Button::new("show-or-premium-show")
                                .label(tr(skin.show_button))
                                .primary()
                                .w_full()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.show_mine(kind, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::ShowOrPremium,
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
                        .child(tr(skin.or))
                        .child(line()),
                )
                .child(title(skin.premium_title))
                .child(about(skin.premium_about))
                .into_any_element();
            let premium_ref = skin.premium_ref;
            let footer = div().flex().w_full().child(
                div()
                    .flex_1()
                    .debug_selector(|| "show-or-premium-premium".to_string())
                    .child(
                        Button::new("show-or-premium-premium")
                            .label(tr(skin.premium_button))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_show_or_premium(cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::ShowOrPremium,
                                    window,
                                    cx,
                                );
                                this.open_premium_from(premium_ref, cx);
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

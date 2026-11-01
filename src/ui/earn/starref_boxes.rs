//! The affiliate boxes (tdesktop `info/bot/starref/info_bot_starref_common`):
//! the setup's confirmations, "View existing programs", joining a
//! program, a joined program's link and "Revoke Link".

use super::join::ProgramRow;
use super::setup::STARREF_TOS_URL;
use super::*;
use crate::ui::os_open::open_link_in;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::{Dialog, DialogContent};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use quill::earn::*;
use quill::lang::{keys, tr, tr_with, trn_with};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The one-shot body the kit dialog content closure takes.
fn one_shot(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static {
    let body = Rc::new(RefCell::new(Some(body)));
    crate::ui::shell::scrollable_dialog_content(move |content, _, _| {
        let body = body
            .borrow_mut()
            .take()
            .unwrap_or_else(|| div().into_any_element());
        content.child(body)
    })
}

/// A centered box title (tdesktop `st::boxTitle` with `al_top`).
fn box_title(text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .justify_center()
        .text_lg()
        .font_semibold()
        .child(text.into())
}

/// A centered paragraph (tdesktop `st::starrefCenteredText`).
fn centered(text: impl Into<SharedString>) -> Div {
    div().text_sm().text_center().child(text.into())
}

/// A table row of the confirmation (tdesktop `giveawayGiftCodeTable`).
fn table_row(label: &'static str, value: String, cx: &App) -> Div {
    div()
        .flex()
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .w(px(120.))
                .p_2()
                .bg(cx.theme().secondary)
                .text_sm()
                .child(label),
        )
        .child(div().flex_1().p_2().text_sm().child(value))
}

/// The join box's recipients: the user's list without the bot and with
/// the chosen recipient, sorted by name (tdesktop `JoinStarRefBox`).
fn recipient_list(
    session: &Session,
    recipients: &[EarnPeer],
    bot_user_id: i64,
    chosen: EarnPeer,
) -> Vec<EarnPeer> {
    if recipients.is_empty() {
        return Vec::new();
    }
    let mut list: Vec<EarnPeer> = recipients
        .iter()
        .copied()
        .filter(|peer| *peer != EarnPeer::User(bot_user_id))
        .collect();
    if !list.contains(&chosen) {
        list.insert(0, chosen);
    }
    list.sort_by_cached_key(|peer| peer_name(session, *peer).to_lowercase());
    list
}

/// The link box's footer (tdesktop `lng_star_ref_link_copy_users`).
/// How long the link icon plays.
const LINK_PLAY: Duration = Duration::from_millis(700);

/// Ease out with a small overshoot past the end.
fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    let c1 = 1.70158;
    let c3 = c1 + 1.;
    1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2)
}

fn opened_through(users: i64, app: &str) -> String {
    match users {
        0 => tr_with(keys::STAR_REF_LINK_COPY_NONE, &[("app", app)]),
        1 => tr_with(keys::Q_MONEY_1_USER_HAVE_OPENED_THROUGH, &[("app", app)]),
        n => trn_with(
            keys::Q_MONEY_USERS_HAVE_OPENED_THROUGH_THIS,
            n,
            &[("app", app)],
        ),
    }
}

impl QuillApp {
    fn close_setup_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(setup) = self.earn_mut().and_then(|e| e.setup.as_mut()) {
            setup.confirm = None;
        }
        cx.notify();
    }

    /// `ConfirmUpdate` and `ConfirmEndBox`.
    pub(super) fn build_earn_setup_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnSetupConfirm, |this, _, cx| {
                this.close_setup_confirm(cx);
            });
        app.update(cx, |this, cx| {
            let Some(setup) = this.earn_state().and_then(|e| e.setup.clone()) else {
                return dialog.on_close(on_close);
            };
            let Some(confirm) = setup.confirm else {
                return dialog.on_close(on_close);
            };
            let exists = setup.exists();
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(box_title(tr(keys::Q_MONEY_WARNING)));
            let action = match confirm {
                SetupConfirm::Save => {
                    body = body
                        .child(div().text_sm().child(if exists {
                            tr(keys::STAR_REF_WARNING_CHANGE)
                        } else {
                            tr(keys::STAR_REF_WARNING_TEXT)
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .rounded_md()
                                .overflow_hidden()
                                .child(table_row(
                                    tr(keys::Q_MONEY_COMMISSION),
                                    format_commission(setup.params.commission_per_mille),
                                    cx,
                                ))
                                .child(table_row(
                                    tr(keys::Q_MONEY_DURATION),
                                    format_program_duration(setup.params.month_count),
                                    cx,
                                )),
                        );
                    Button::new("earn-setup-confirm")
                        .label(if exists {
                            tr(keys::Q_MONEY_UPDATE)
                        } else {
                            tr(keys::Q_MONEY_START)
                        })
                        .primary()
                        .loading(setup.saving)
                        .disabled(setup.saving)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.earn_call(
                                tr(keys::Q_MONEY_COULDNT_SAVE_THE_AFFILIATE_PROGRAM),
                                cx,
                                |driver| driver.save_starref_program(),
                            );
                        }))
                }
                SetupConfirm::End => {
                    body = body
                        .child(div().text_sm().child(tr(keys::STAR_REF_WARNING_IF_END)))
                        .child(Self::bullet(tr(
                            keys::Q_MONEY_ANY_REFERRAL_LINKS_ALREADY_SHARED,
                        )))
                        .child(Self::bullet(tr(keys::STAR_REF_WARNING_IF_END2)))
                        .child(Self::bullet(tr(keys::Q_MONEY_YOU_WILL_BE_ABLE_TO)));
                    Button::new("earn-setup-end-confirm")
                        .label(tr(keys::Q_MONEY_END_ANYWAY))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            // tdesktop closes this box before the request.
                            this.earn_call(
                                tr(keys::Q_MONEY_COULDNT_END_THE_AFFILIATE_PROGRAM),
                                cx,
                                |driver| driver.end_starref_program(),
                            );
                            this.close_setup_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::EarnSetupConfirm, window, cx);
                        }))
                }
            };
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .w_full()
                .child(
                    Button::new("earn-setup-cancel")
                        .debug_selector(|| "earn-setup-cancel".into())
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_setup_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::EarnSetupConfirm, window, cx);
                        })),
                )
                .child(action);
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// A dotted point of `ConfirmEndBox`.
    fn bullet(text: &'static str) -> Div {
        div()
            .flex()
            .gap_2()
            .pl_1()
            .text_sm()
            .child(
                div()
                    .mt(px(7.))
                    .size(px(5.))
                    .flex_none()
                    .rounded_full()
                    .bg(text_muted()),
            )
            .child(div().flex_1().child(text))
    }

    fn close_programs_box(&mut self, cx: &mut Context<Self>) {
        if let Some(earn) = self.earn_mut() {
            earn.programs_box = None;
        }
        cx.notify();
    }

    /// "View existing programs" (tdesktop `ProgramsListBox`).
    pub(super) fn build_earn_programs_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnPrograms, |this, _, cx| {
                this.close_programs_box(cx);
            });
        app.update(cx, |this, cx| {
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let earn = &session.payments.earn;
            let (Some(list), Some(bot)) = (
                earn.programs_box.as_ref(),
                earn.setup.as_ref().map(|s| s.bot_user_id),
            ) else {
                return dialog.on_close(on_close);
            };
            let mut rows = div()
                .id("earn-programs-list")
                .max_h(px(420.))
                .overflow_y_scroll()
                .track_scroll(&this.earn.box_scroll)
                .flex()
                .flex_col();
            // tdesktop `setupAddForBot`: a bot without a program of its
            // own is offered to start one.
            let has_program = session
                .bots
                .bot_info
                .get(&bot)
                .and_then(|info| info.as_ref()?.affiliate_program)
                .is_some_and(|p| p.params.commission_per_mille > 0);
            if list.loaded && !has_program {
                rows = rows.child(
                    div()
                        .id("earn-programs-add")
                        .flex()
                        .items_center()
                        .gap_3()
                        .py_2()
                        .px_2()
                        .rounded_md()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .tab_index(0)
                        .hover(|s| s.bg(cx.theme().secondary))
                        .text_color(cx.theme().primary)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_programs_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::EarnPrograms, window, cx);
                        }))
                        .child(Icon::new(IconName::Plus).size(px(20.)))
                        .child(div().text_sm().font_semibold().child(tr_with(
                            keys::Q_MONEY_ADD_2,
                            &[(
                                "peer_name",
                                &(peer_name(session, EarnPeer::User(bot))).to_string(),
                            )],
                        ))),
                );
            }
            for (index, program) in list.programs.iter().enumerate() {
                rows = rows.child(this.program_row(
                    session,
                    ("earn-box-program", index),
                    ProgramRow::found(program),
                    true,
                    cx,
                ));
            }
            if !list.loaded {
                rows = rows.child(about(tr(keys::Q_MONEY_LOADING_2)).py_2());
            } else if list.loading {
                rows = rows.child(div().flex().justify_center().py_2().child(Spinner::new()));
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(box_title(tr(keys::Q_MONEY_AFFILIATE_PROGRAMS)))
                .child(rows);
            let footer = div().flex().justify_end().w_full().child(
                Button::new("earn-programs-close")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_programs_box(cx);
                        this.close_kit_dialog_if_done(DialogKind::EarnPrograms, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn close_join_box(&mut self, cx: &mut Context<Self>) {
        if let Some(earn) = self.earn_mut() {
            earn.join_box = None;
            earn.pending_join = None;
        }
        cx.notify();
    }

    /// The bot's photo, an arrow and the recipient's with the Premium
    /// star disc on its corner (tdesktop `CreateUserpicsTransfer` with
    /// `StarRefJoin`: 60 px photos 42 px apart, `starrefJoinIcon` on a
    /// gradient disc ringed in the window color).
    fn transfer_userpics(
        &self,
        session: &Session,
        bot: EarnPeer,
        recipient: EarnPeer,
        cx: &App,
    ) -> Div {
        let disc = div()
            .debug_selector(|| "earn-join-star".into())
            .absolute()
            .right(px(-4.))
            .bottom(px(-2.))
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_2()
            .border_color(crate::ui::chat_theme::bg_canvas())
            .bg(crate::ui::premium_mark::premium_gradient())
            .child(
                Icon::new(IconName::StarFill)
                    .with_size(px(10.))
                    .text_color(cx.theme().primary_foreground),
            );
        div()
            .flex()
            .justify_center()
            .items_center()
            .child(self.earn_avatar(session, bot, 60.))
            .child(
                div().w(px(42.)).flex().justify_center().child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(20.))
                        .text_color(text_muted()),
                ),
            )
            .child(
                div()
                    .relative()
                    .child(self.earn_avatar(session, recipient, 60.))
                    .child(disc),
            )
    }

    /// `JoinStarRefBox`.
    pub(super) fn build_earn_join_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnJoin, |this, _, cx| {
                this.close_join_box(cx);
            });
        app.update(cx, |this, cx| {
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let earn = &session.payments.earn;
            let Some(join) = earn.join_box.clone() else {
                return dialog.on_close(on_close);
            };
            let bot = EarnPeer::User(join.bot_user_id);
            let app_name = peer_name(session, bot);
            let params = join.info.params;
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(this.transfer_userpics(session, bot, join.recipient, cx))
                .child(box_title(tr(keys::Q_MONEY_AFFILIATE_PROGRAM)))
                .child(centered(tr_with(
                    keys::STAR_REF_ONE_ABOUT,
                    &[
                        ("app", &app_name.to_string()),
                        (
                            "amount",
                            &(format_commission(params.commission_per_mille)).to_string(),
                        ),
                        (
                            "duration",
                            &(format_for_program_duration(params.month_count)).to_string(),
                        ),
                    ],
                )));
            let daily = join.info.daily_revenue_per_user;
            if daily.stars > 0 || daily.nanos > 0 {
                body = body.child(
                    div()
                        .flex()
                        .justify_center()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .child(tr(keys::Q_MONEY_DAILY_REVENUE_PER_USER))
                        .child(star_icon(14.))
                        .child(div().font_semibold().child(daily.label())),
                );
            }
            let list = if join.choose_recipient {
                recipient_list(
                    session,
                    earn.recipients.as_deref().unwrap_or(&[]),
                    join.bot_user_id,
                    join.recipient,
                )
            } else {
                Vec::new()
            };
            if !list.is_empty() {
                body = body.child(centered(tr(keys::STAR_REF_LINK_RECIPIENT)));
                let bubble = this.earn_peer_bubble(session, join.recipient, cx);
                if list.len() > 1 {
                    let names: Vec<(EarnPeer, String)> = list
                        .iter()
                        .map(|peer| (*peer, peer_name(session, *peer)))
                        .collect();
                    let current = join.recipient;
                    let owner = cx.entity().downgrade();
                    body = body.child(
                        div().flex().justify_center().child(
                            Button::new("earn-join-recipient")
                                .ghost()
                                .child(
                                    bubble.child(
                                        Icon::new(IconName::ChevronDown)
                                            .size(px(14.))
                                            .text_color(text_muted()),
                                    ),
                                )
                                .dropdown_menu(move |mut menu, _, _| {
                                    for (peer, name) in names.iter().cloned() {
                                        let owner = owner.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(name)
                                                .checked(peer == current)
                                                .on_click(move |_, _, cx| {
                                                    let _ = owner.update(cx, |this, cx| {
                                                        if let Some(join) = this
                                                            .earn_mut()
                                                            .and_then(|e| e.join_box.as_mut())
                                                        {
                                                            join.recipient = peer;
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        ),
                    );
                } else {
                    body = body.child(div().flex().justify_center().child(bubble));
                }
            }
            let footer = div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(
                    Button::new("earn-join-program")
                        .debug_selector(|| "earn-join-program".into())
                        .label(tr(keys::Q_MONEY_JOIN_PROGRAM))
                        .primary()
                        .w_full()
                        .loading(join.joining)
                        .disabled(join.joining)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.earn_call(
                                tr(keys::Q_MONEY_COULDNT_JOIN_THE_PROGRAM),
                                cx,
                                |driver| driver.join_affiliate_program(),
                            );
                        })),
                )
                .child(about_link(
                    "earn-join-terms",
                    tr(keys::Q_MONEY_BY_JOINING_THIS_PROGRAM_YOU),
                    tr(keys::Q_MONEY_TERMS_AND_CONDITIONS),
                    tr(keys::Q_MONEY_OF_AFFILIATE_PROGRAMS),
                    {
                        let app = app.downgrade();
                        move |_, cx| open_link_in(&app, STARREF_TOS_URL, cx)
                    },
                    cx,
                ));
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn close_link_box(&mut self, cx: &mut Context<Self>) {
        if let Some(earn) = self.earn_mut() {
            earn.link_box = None;
        }
        cx.notify();
    }

    fn copy_affiliate_link(&mut self, url: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        self.connection
            .set_note(StatusNote::info(tr(keys::STICKERS_COPIED)));
        cx.notify();
    }

    /// The link icon's one-time play as the box opens: it swings in from
    /// a quarter turn and settles with a small overshoot (tdesktop plays
    /// the `starref_link` animation once in `CreateLinkIcon`).
    fn link_icon_motion(&self, cx: &mut Context<Self>) -> Transformation {
        let elapsed = self.earn.link_shown.map_or(LINK_PLAY, |since| {
            crate::ui::motion::elapsed_since(since, Instant::now())
        });
        let t = if crate::ui::motion::interface_animations_off() {
            1.
        } else {
            crate::ui::motion::progress(elapsed, LINK_PLAY)
        };
        if t < 1. {
            self.request_animation_tick(60, cx);
        }
        let eased = ease_out_back(t);
        Transformation::rotate(radians(-(1. - eased) * std::f32::consts::FRAC_PI_2))
            .with_scaling(size(0.6 + 0.4 * eased, 0.6 + 0.4 * eased))
    }

    /// `StarRefLinkBox`.
    pub(super) fn build_earn_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnLink, |this, _, cx| {
                this.close_link_box(cx);
            });
        app.update(cx, |this, cx| {
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let Some(link) = session.payments.earn.link_box.clone() else {
                return dialog.on_close(on_close);
            };
            let bot = EarnPeer::User(link.program.bot_user_id);
            let app_name = peer_name(session, bot);
            let params = link.program.params;
            let users = link.program.user_count;
            let audience = match link.recipient {
                EarnPeer::Chat(_) => "subscribers",
                EarnPeer::User(_) => "friends",
            };
            let mut icon = div().flex().flex_col().items_center().child(
                div()
                    .size(px(72.))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::Link)
                            .size(px(34.))
                            .text_color(cx.theme().primary_foreground)
                            .transform(this.link_icon_motion(cx)),
                    ),
            );
            if users > 0 {
                icon = icon.child(
                    div()
                        .mt(px(-10.))
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .px_1p5()
                        .rounded_full()
                        .border_2()
                        .border_color(cx.theme().background)
                        .bg(cx.theme().primary)
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().primary_foreground)
                        .child(Icon::new(IconName::User).size(px(10.)))
                        .child(crate::ui::statistics::format_stat_count(users as f64)),
                );
            }
            let url = link.program.url.clone();
            let preview_url = url.clone();
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(icon)
                .child(box_title(tr(keys::Q_MONEY_REFERRAL_LINK)))
                .child(centered(tr_with(
                    keys::Q_MONEY_SHARE_THIS_LINK_WITH_YOUR,
                    &[
                        ("audience", audience),
                        (
                            "commission_per_mille",
                            &(format_commission(params.commission_per_mille)).to_string(),
                        ),
                        ("app_name", &app_name.to_string()),
                        (
                            "month_count",
                            &(format_for_program_duration(params.month_count)).to_string(),
                        ),
                    ],
                )))
                .child(centered(tr(keys::STAR_REF_LINK_RECIPIENT)))
                .child(div().flex().justify_center().child(this.earn_peer_bubble(
                    session,
                    link.recipient,
                    cx,
                )))
                .child(
                    div()
                        .id("earn-link-preview")
                        .p_2()
                        .rounded_lg()
                        .bg(cx.theme().secondary)
                        .text_sm()
                        .text_center()
                        .truncate()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(tr(keys::CONTEXT_COPY_LINK))
                        .tab_index(0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_affiliate_link(&preview_url, cx);
                        }))
                        .child(url.trim_start_matches("https://").to_string()),
                );
            let footer = div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(
                    Button::new("earn-link-copy")
                        .debug_selector(|| "earn-link-copy".into())
                        .label(tr(keys::CONTEXT_COPY_LINK))
                        .icon(IconName::Copy)
                        .primary()
                        .w_full()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copy_affiliate_link(&url, cx);
                            this.close_link_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::EarnLink, window, cx);
                        })),
                )
                .child(about(opened_through(users, &app_name)).text_center());
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn close_revoke(&mut self, cx: &mut Context<Self>) {
        if let Some(earn) = self.earn_mut() {
            earn.revoke = None;
        }
        cx.notify();
    }

    /// "Revoke Link" (tdesktop's `rowContextMenu` → `MakeConfirmBox`).
    pub(super) fn build_earn_revoke_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnRevoke, |this, _, cx| {
                this.close_revoke(cx);
            });
        app.update(cx, |this, cx| {
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let Some(revoke) = session.payments.earn.revoke.clone() else {
                return dialog.on_close(on_close);
            };
            let bot = peer_name(session, EarnPeer::User(revoke.program.bot_user_id));
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_lg()
                        .font_semibold()
                        .child(tr(keys::Q_MONEY_REVOKE_LINK)),
                )
                .child(div().text_sm().child(tr_with(
                    keys::STAR_REF_REVOKE_TEXT,
                    &[("bot", &bot.to_string())],
                )));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .w_full()
                .child(
                    Button::new("earn-revoke-cancel")
                        .debug_selector(|| "earn-revoke-cancel".into())
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_revoke(cx);
                            this.close_kit_dialog_if_done(DialogKind::EarnRevoke, window, cx);
                        })),
                )
                .child(
                    Button::new("earn-revoke-ok")
                        .debug_selector(|| "earn-revoke-ok".into())
                        .label(tr(keys::BOX_OK))
                        .primary()
                        .loading(revoke.sending)
                        .disabled(revoke.sending)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.earn_call(
                                tr(keys::Q_MONEY_COULDNT_REVOKE_THE_LINK),
                                cx,
                                |driver| driver.revoke_affiliate_link(),
                            );
                        })),
                );
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The setup page's "Warning" boxes.
    EarnSetupConfirm => DialogSpec::new(
        7615,
        |app| app.earn_state().and_then(|e| e.setup.as_ref()).is_some_and(|s| s.confirm.is_some()),
        QuillApp::build_earn_setup_confirm_dialog,
    )
    .closes(|app, _, cx| app.close_setup_confirm(cx)),
    /// "View existing programs".
    EarnPrograms => DialogSpec::new(
        7616,
        |app| app.earn_state().is_some_and(|e| e.programs_box.is_some()),
        QuillApp::build_earn_programs_dialog,
    ),
    /// Joining a program.
    EarnJoin => DialogSpec::new(
        7617,
        |app| app.earn_state().is_some_and(|e| e.join_box.is_some()),
        QuillApp::build_earn_join_dialog,
    ),
    /// A joined program's referral link.
    EarnLink => DialogSpec::new(
        7618,
        |app| app.earn_state().is_some_and(|e| e.link_box.is_some()),
        QuillApp::build_earn_link_dialog,
    ),
    /// "Revoke Link".
    EarnRevoke => DialogSpec::new(
        7619,
        |app| app.earn_state().is_some_and(|e| e.revoke.is_some()),
        QuillApp::build_earn_revoke_dialog,
    ),
}

#[cfg(test)]
mod unit {
    use super::opened_through;

    #[test]
    fn the_link_footer_counts_users() {
        assert_eq!(
            opened_through(0, "Bot"),
            "No one have opened Bot through this link."
        );
        assert_eq!(
            opened_through(1, "Bot"),
            "1 user have opened Bot through this link."
        );
        assert_eq!(
            opened_through(5, "Bot"),
            "5 users have opened Bot through this link."
        );
    }
}

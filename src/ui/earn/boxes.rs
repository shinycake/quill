//! The revenue pages' boxes: "Learn more" (tdesktop `addAboutWithLearn`),
//! a TON transaction's details, a Stars receipt and the boost box behind
//! a locked "Switch Off Ads" (`AskBoostBox` with `AskBoostCpm`).

use super::revenue::ton_amount;
use super::*;
use crate::ui::chat_theme::{danger, success};
use crate::ui::earn_state::AskBoostReason;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::{Dialog, DialogContent};
use quill::boost_box::BoostCounters;
use quill::lang::{keys, tr, tr_with, trn};
use std::cell::RefCell;
use std::rc::Rc;

/// tdesktop `lng_channel_earn_learn_coin_link`.
const TON_BLOG_URL: &str = "https://telegram.org/blog/monetization-for-channels";

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

/// tdesktop's diamonds easter egg on the learn box's TON emoji.
fn diamonds_toast(count: u32) -> String {
    match count {
        100.. => tr(keys::Q_MONEY_YOU_ARE_RICH_NOW).into(),
        1 => tr(keys::Q_MONEY_YOU_HAVE_EARNED_1_DIAMOND).into(),
        n => trn(keys::Q_MONEY_YOU_HAVE_EARNED_DIAMONDS, n),
    }
}

impl QuillApp {
    fn close_earn_learn(&mut self, cx: &mut Context<Self>) {
        self.earn.learn = false;
        cx.notify();
    }

    pub(super) fn build_earn_learn_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnLearn, |this, _, cx| {
                this.close_earn_learn(cx);
            });
        app.update(cx, |this, cx| {
            let is_bot = this
                .earn_state()
                .and_then(|e| e.page.as_ref())
                .is_some_and(|p| matches!(p.peer, EarnPeer::User(_)));
            let primary = cx.theme().primary;
            let body = div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div().flex().justify_center().child(
                        div()
                            .size(px(64.))
                            .rounded_full()
                            .bg(primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Megaphone)
                                    .size(px(30.))
                                    .text_color(cx.theme().primary_foreground),
                            ),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .text_lg()
                        .font_semibold()
                        .child(if is_bot {
                            tr(keys::CHANNEL_EARN_BOT_LEARN_TITLE)
                        } else {
                            tr(keys::CHANNEL_EARN_LEARN_TITLE)
                        }),
                )
                .child(info_point(
                    IconName::Megaphone,
                    tr(keys::CHANNEL_EARN_LEARN_IN_SUBTITLE),
                    if is_bot {
                        tr(keys::CHANNEL_EARN_LEARN_BOT_IN_ABOUT)
                    } else {
                        tr(keys::CHANNEL_EARN_LEARN_IN_ABOUT)
                    },
                    cx,
                ))
                .child(info_point(
                    IconName::ChartPie,
                    tr(keys::CHANNEL_EARN_LEARN_SPLIT_SUBTITLE),
                    tr(keys::CHANNEL_EARN_LEARN_SPLIT_ABOUT),
                    cx,
                ))
                .child(info_point(
                    IconName::ArrowUpRight,
                    tr(keys::CHANNEL_EARN_LEARN_OUT_SUBTITLE),
                    tr(keys::CHANNEL_EARN_LEARN_OUT_ABOUT),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .items_center()
                        .gap_1()
                        .text_lg()
                        .font_semibold()
                        .child(tr(keys::Q_MONEY_WHAT_IS))
                        .child(
                            div()
                                .id("earn-learn-diamond")
                                .debug_selector(|| "earn-learn-diamond".into())
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.earn.diamonds < 100 {
                                        this.earn.diamonds += 1;
                                    }
                                    this.connection.set_note(StatusNote::info(diamonds_toast(
                                        this.earn.diamonds,
                                    )));
                                    cx.notify();
                                }))
                                .child(ton_icon(20., primary)),
                        )
                        .child(tr(keys::Q_MONEY_TON)),
                )
                .child(super::revenue::about_with_link(
                    "earn-learn-coin",
                    tr(keys::Q_MONEY_TON_IS_A_BLOCKCHAIN_PLATFORM),
                    |_, cx| cx.open_url(TON_BLOG_URL),
                    cx,
                ))
                .into_any_element();
            let footer = div().flex().w_full().child(
                Button::new("earn-learn-close")
                    .debug_selector(|| "earn-learn-close".into())
                    .label(tr(keys::CHANNEL_EARN_LEARN_CLOSE))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_earn_learn(cx);
                        this.close_kit_dialog_if_done(DialogKind::EarnLearn, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .content(one_shot(body))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn close_earn_tx(&mut self, cx: &mut Context<Self>) {
        self.earn.ton_tx = None;
        self.earn.star_tx = None;
        cx.notify();
    }

    /// A TON history row's box (tdesktop `detailsBox`).
    pub(super) fn build_earn_ton_tx_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnTonTx, |this, _, cx| {
                this.close_earn_tx(cx);
            });
        app.update(cx, |this, cx| {
            let (Some(tx), Some(session)) = (this.earn.ton_tx.clone(), this.session()) else {
                return dialog.on_close(on_close);
            };
            let incoming = tx.is_incoming();
            let color: Hsla = if incoming { success() } else { danger() }.into();
            let date = tx.date_line();
            let mut body = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(ton_amount(tx.amount.abs(), 26., color, Some(incoming)))
                .child(
                    div()
                        .text_xs()
                        .text_color(if tx.failed() { danger() } else { text_muted() })
                        .child(date),
                )
                .child(div().pt_2().text_sm().font_semibold().child(if incoming {
                    tr(keys::CHANNEL_EARN_HISTORY_IN_ABOUT)
                } else {
                    tr(keys::CHANNEL_EARN_HISTORY_OUT)
                }));
            if incoming
                && let Some(peer) = this
                    .earn_state()
                    .and_then(|e| e.page.as_ref())
                    .map(|p| p.peer)
            {
                body = body.child(this.earn_peer_bubble(session, peer, cx));
            }
            let explorer = tx.explorer_url().map(str::to_string);
            let button = match explorer {
                Some(url) => Button::new("earn-tx-explorer")
                    .label(tr(keys::CHANNEL_EARN_HISTORY_OUT_BUTTON))
                    .primary()
                    .w_full()
                    .on_click(move |_, _, cx| cx.open_url(&url)),
                None => Button::new("earn-tx-ok")
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_earn_tx(cx);
                        this.close_kit_dialog_if_done(DialogKind::EarnTonTx, window, cx);
                    })),
            };
            dialog
                .overlay(true)
                .content(one_shot(body.into_any_element()))
                .footer(div().flex().w_full().child(button))
                .on_close(on_close)
        })
    }

    /// A Stars history row's receipt (tdesktop `ReceiptCreditsBox`).
    pub(super) fn build_earn_star_tx_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnStarTx, |this, _, cx| {
                this.close_earn_tx(cx);
            });
        app.update(cx, |this, cx| {
            let (Some(tx), Some(session)) = (this.earn.star_tx.clone(), this.session()) else {
                return dialog.on_close(on_close);
            };
            let body = crate::ui::premium_ui::star_tx_facts(session, &tx, cx);
            let footer = div().flex().w_full().child(
                Button::new("earn-receipt-ok")
                    .label(tr(keys::BOX_OK))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_earn_tx(cx);
                        this.close_kit_dialog_if_done(DialogKind::EarnStarTx, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .content(one_shot(body))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn close_earn_ask_boost(&mut self, cx: &mut Context<Self>) {
        self.earn.ask_boost = None;
        cx.notify();
    }

    /// "Boost Channel": the level "Switch Off Ads" needs and the link to
    /// share (tdesktop `AskBoostBox`).
    pub(super) fn build_earn_ask_boost_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EarnAskBoost, |this, _, cx| {
                this.close_earn_ask_boost(cx);
            });
        app.update(cx, |this, cx| {
            let (Some(ask), Some(session)) = (this.earn.ask_boost, this.session()) else {
                return dialog.on_close(on_close);
            };
            let (chat_id, level) = (ask.chat_id, ask.level);
            let (title, needs) = match ask.reason {
                AskBoostReason::Cpm => (
                    tr(keys::Q_MONEY_BOOST_CHANNEL).to_string(),
                    tr_with(
                        keys::Q_MONEY_YOUR_CHANNEL_NEEDS_TO_REACH,
                        &[("level", &level.to_string())],
                    ),
                ),
                AskBoostReason::Autotranslate => (
                    tr(keys::BOOST_CHANNEL_TITLE_AUTOTRANSLATE).to_string(),
                    trn(keys::BOOST_CHANNEL_NEEDS_LEVEL_AUTOTRANSLATE, level).replace("**", ""),
                ),
            };
            let link = session
                .groups
                .chat_boost_links
                .get(&chat_id)
                .map(|(link, _)| link.clone())
                .unwrap_or_default();
            let mut body = div().flex().flex_col().gap_3();
            if let Some(status) = session.groups.chat_boost_details.get(&chat_id) {
                let counters = BoostCounters::from(status);
                body = body.child(Self::boost_progress(&counters, cx));
            }
            body = body
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .text_lg()
                        .font_semibold()
                        .child(title),
                )
                .child(div().text_sm().text_center().child(needs))
                .child(
                    div()
                        .text_sm()
                        .text_center()
                        .child(tr(keys::Q_MONEY_ASK_YOUR_PREMIUM_SUBSCRIBERS_TO)),
                )
                .child(
                    div()
                        .id("earn-boost-link")
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .text_sm()
                        .text_center()
                        .truncate()
                        .child(if link.is_empty() {
                            "\u{2026}".to_string()
                        } else {
                            link.clone()
                        }),
                );
            if let Some(features) = Self::boost_features_for(session, chat_id, false, level) {
                body = body.child(features);
            }
            let footer = div().flex().w_full().child(
                Button::new("earn-boost-copy")
                    .label(tr(keys::CONTEXT_COPY_LINK))
                    .primary()
                    .w_full()
                    .disabled(link.is_empty())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                        this.connection
                            .set_note(StatusNote::info(tr(keys::STICKERS_COPIED)));
                        cx.notify();
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
    /// The Monetization page's "Learn more" box.
    EarnLearn => DialogSpec::new(7610, |app| app.earn.learn, QuillApp::build_earn_learn_dialog),
    /// A TON transaction's details.
    EarnTonTx => DialogSpec::new(
        7611,
        |app| app.earn.ton_tx.is_some(),
        QuillApp::build_earn_ton_tx_dialog,
    )
    .closes(|app, _, cx| app.close_earn_tx(cx)),
    /// A Stars transaction's receipt from a revenue page.
    EarnStarTx => DialogSpec::new(
        7613,
        |app| app.earn.star_tx.is_some(),
        QuillApp::build_earn_star_tx_dialog,
    )
    .closes(|app, _, cx| app.close_earn_tx(cx)),
    /// The boost box of a locked "Switch Off Ads".
    EarnAskBoost => DialogSpec::new(
        7614,
        |app| app.earn.ask_boost.is_some(),
        QuillApp::build_earn_ask_boost_dialog,
    ),
}

#[cfg(test)]
mod unit {
    use super::diamonds_toast;

    #[test]
    fn diamonds_count_up_to_rich() {
        assert_eq!(diamonds_toast(1), "You have earned 1 diamond!");
        assert_eq!(diamonds_toast(7), "You have earned 7 diamonds!");
        assert_eq!(diamonds_toast(100), "You are rich now!");
    }
}

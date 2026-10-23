//! Where the revenue and affiliate pages open from: the channel and bot
//! profiles (tdesktop `info_profile_actions`), the bot and channel edit
//! boxes (`edit_peer_info_box`) and the Stars dialog
//! (`settings_credits` `BuildCreditsButtons`).

use super::*;
use crate::ui::pressable::action_row;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use quill::earn::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::StarAmount;
use quill::telegram::envelope::ChatKind;

/// A balance row's right side: the TON and Stars amounts that are above
/// zero (tdesktop's `CreateRightLabel` on the profile's Balance row).
fn balance_label(ton: Option<i64>, stars: Option<StarAmount>, cx: &App) -> Div {
    let mut label = div()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .text_color(text_muted());
    if let Some(nanos) = ton.filter(|n| *n > 0) {
        label = label
            .child(ton_icon(12., cx.theme().primary))
            .child(ton_label(nanos));
    }
    if let Some(amount) = stars.filter(|a| a.stars > 0 || a.nanos > 0) {
        label = label.child(star_icon(12.)).child(amount.label());
    }
    label
}

fn positive_ton(ton: Option<i64>) -> bool {
    ton.is_some_and(|n| n > 0)
}

fn positive_stars(stars: Option<StarAmount>) -> bool {
    stars.is_some_and(|a| a.stars > 0 || a.nanos > 0)
}

impl QuillApp {
    /// A bot edit row's click: the page opens in the info panel and the
    /// box closes (tdesktop `showSection` from the box).
    fn leave_bot_edit_for(
        panel: EarnPanel,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _, window, cx| {
            this.close_bot_edit(cx);
            this.close_kit_dialog_if_done(crate::ui::shell::DialogKind::BotEdit, window, cx);
            this.open_earn(panel, window, cx);
        })
    }

    /// A row with a label on the left and a value on the right.
    fn earn_value_row(
        id: impl Into<ElementId>,
        icon: IconName,
        label: &'static str,
        value: Div,
        cx: &App,
    ) -> gpui_kit::Stateful<Div> {
        action_row(id, Some(icon), label, false, cx)
            .child(div().flex_1())
            .child(value)
    }

    /// The channel profile's "Balance" row, shown once a balance is above
    /// zero (tdesktop `canViewBalance` with the balances toggled on).
    pub(in crate::ui) fn channel_balance_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let info = session
            .chat_supergroup(chat_id)
            .and_then(|id| session.supergroup_full_info(id))?;
        if !info.admin.can_get_revenue_statistics && !info.admin.can_get_star_revenue_statistics {
            return None;
        }
        let peer = EarnPeer::Chat(chat_id.0);
        let earn = &session.payments.earn;
        let (ton, stars) = (earn.ton_balance(peer), earn.star_balance(peer));
        if !positive_ton(ton) && !positive_stars(stars) {
            return None;
        }
        Some(
            Self::earn_value_row(
                "info-panel-balance",
                IconName::Coins,
                tr(keys::Q_MONEY_BALANCE),
                balance_label(ton, stars, cx),
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_earn(
                    EarnPanel::Revenue {
                        peer,
                        screen: EarnScreen::Monetization,
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element(),
        )
    }

    /// A bot profile's "Affiliate Program" row and "Balance" section
    /// (tdesktop `addAffiliateProgram` and `addBalanceActions`).
    pub(in crate::ui) fn bot_earn_rows(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let mut column = div().flex().flex_col().gap_0p5();
        let mut any = false;
        let program = session
            .bots
            .bot_info
            .get(&user_id)
            .and_then(|info| info.as_ref()?.affiliate_program)
            .filter(|p| p.params.commission_per_mille > 0);
        if let Some(program) = program {
            any = true;
            let commission = format_commission(program.params.commission_per_mille);
            let name = peer_name(session, EarnPeer::User(user_id));
            column = column
                .child(
                    Self::earn_value_row(
                        "info-panel-affiliate",
                        IconName::Share2,
                        tr(keys::Q_MONEY_AFFILIATE_PROGRAM),
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child(commission.clone()),
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.earn_call(
                            tr(keys::Q_MONEY_COULDNT_OPEN_THE_AFFILIATE_PROGRAM),
                            cx,
                            |driver| driver.open_bot_affiliate_join(user_id),
                        );
                    })),
                )
                .child(
                    about(tr_with(
                        keys::MANAGE_PEER_BOT_STAR_REF_ABOUT,
                        &[
                            ("bot", &name.to_string()),
                            ("amount", &commission.to_string()),
                        ],
                    ))
                    .px_2(),
                );
        }
        let peer = EarnPeer::User(user_id);
        let earn = &session.payments.earn;
        let (ton, stars) = (earn.ton_balance(peer), earn.star_balance(peer));
        if positive_ton(ton) || positive_stars(stars) {
            any = true;
            column = column.child(header(tr(keys::Q_MONEY_BALANCE), cx).px_2());
            if positive_ton(ton) {
                column = column.child(
                    Self::earn_value_row(
                        "info-panel-balance-ton",
                        IconName::Gem,
                        tr(keys::Q_MONEY_TONCOIN),
                        balance_label(ton, None, cx),
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_earn(
                            EarnPanel::Revenue {
                                peer,
                                screen: EarnScreen::Monetization,
                            },
                            window,
                            cx,
                        );
                    })),
                );
            }
            if positive_stars(stars) {
                column = column.child(
                    Self::earn_value_row(
                        "info-panel-balance-stars",
                        IconName::Star,
                        tr(keys::Q_MONEY_STARS),
                        balance_label(None, stars, cx),
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_earn(
                            EarnPanel::Revenue {
                                peer,
                                screen: EarnScreen::Stars,
                            },
                            window,
                            cx,
                        );
                    })),
                );
            }
        }
        any.then(|| column.into_any_element())
    }

    /// The bot edit box's "Toncoin", "Stars" and "Affiliate Program" rows
    /// (tdesktop `fillBotCurrencyButton`, `fillBotCreditsButton`,
    /// `fillBotAffiliateProgram`).
    pub(in crate::ui) fn bot_edit_earn_rows(
        &self,
        bot_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let peer = EarnPeer::User(bot_id);
        let earn = &session.payments.earn;
        let (ton, stars) = (earn.ton_balance(peer), earn.star_balance(peer));
        let mut column = div().flex().flex_col().gap_0p5();
        let mut any = false;
        if positive_ton(ton) {
            any = true;
            column = column.child(
                Self::earn_value_row(
                    "bot-edit-ton",
                    IconName::Gem,
                    tr(keys::Q_MONEY_TONCOIN),
                    balance_label(ton, None, cx),
                    cx,
                )
                .on_click(Self::leave_bot_edit_for(
                    EarnPanel::Revenue {
                        peer,
                        screen: EarnScreen::Monetization,
                    },
                    cx,
                )),
            );
        }
        if positive_stars(stars) {
            any = true;
            column = column.child(
                Self::earn_value_row(
                    "bot-edit-stars",
                    IconName::Star,
                    tr(keys::Q_MONEY_STARS),
                    balance_label(None, stars, cx),
                    cx,
                )
                .on_click(Self::leave_bot_edit_for(
                    EarnPanel::Revenue {
                        peer,
                        screen: EarnScreen::Stars,
                    },
                    cx,
                )),
            );
        }
        if session.earn_setup_allowed(bot_id) {
            any = true;
            let commission = session
                .bots
                .bot_info
                .get(&bot_id)
                .and_then(|info| info.as_ref()?.affiliate_program)
                .map_or(0, |p| p.params.commission_per_mille);
            let value = if commission > 0 {
                format_commission(commission)
            } else {
                tr(keys::Q_MONEY_OFF).into()
            };
            column = column.child(
                Self::earn_value_row(
                    "bot-edit-affiliate",
                    IconName::Share2,
                    tr(keys::Q_MONEY_AFFILIATE_PROGRAM),
                    div().text_sm().text_color(text_muted()).child(value),
                    cx,
                )
                .on_click(Self::leave_bot_edit_for(
                    EarnPanel::Setup {
                        bot_user_id: bot_id,
                    },
                    cx,
                )),
            );
        }
        any.then(|| column.into_any_element())
    }

    /// The channel edit box's "Affiliate programs" (tdesktop
    /// `hasStarRef`: join allowed, a broadcast channel, may post).
    pub(in crate::ui) fn channel_affiliate_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let broadcast = session.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
        if !broadcast || !session.earn_join_allowed(EarnPeer::Chat(chat_id.0)) {
            return None;
        }
        Some(
            action_row(
                "gl-affiliate-programs",
                Some(IconName::Share2),
                tr(keys::Q_MONEY_AFFILIATE_PROGRAMS_2),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_group_edit_box(cx);
                this.close_kit_dialog_if_done(
                    crate::ui::shell::DialogKind::GroupEditBox,
                    window,
                    cx,
                );
                this.open_earn(
                    EarnPanel::Join {
                        peer: EarnPeer::Chat(chat_id.0),
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element(),
        )
    }

    /// The Stars dialog's "View Statistics": shown once the user's own
    /// Stars revenue statistics answered (tdesktop `statsEnabled`).
    pub(in crate::ui) fn stars_stats_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let me = session.my_user_id?;
        if session.payments.earn.self_stats != Some(true) {
            return None;
        }
        Some(
            Button::new("stars-view-statistics")
                .label(tr(keys::Q_MONEY_VIEW_STATISTICS))
                .icon(IconName::ChartColumn)
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.close_stars(cx);
                    this.close_kit_dialog_if_done(crate::ui::shell::DialogKind::Stars, window, cx);
                    this.open_earn(
                        EarnPanel::Revenue {
                            peer: EarnPeer::User(me),
                            screen: EarnScreen::Stars,
                        },
                        window,
                        cx,
                    );
                }))
                .into_any_element(),
        )
    }

    /// The Stars dialog's "Earn Stars from Mini Apps" (tdesktop
    /// `Join::Allowed(self)`).
    pub(in crate::ui) fn stars_earn_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let me = session.my_user_id?;
        if !session.earn_join_allowed(EarnPeer::User(me)) {
            return None;
        }
        Some(
            Button::new("stars-earn")
                .label(tr(keys::CREDITS_EARN_BUTTON))
                .icon(IconName::Share2)
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.close_stars(cx);
                    this.close_kit_dialog_if_done(crate::ui::shell::DialogKind::Stars, window, cx);
                    this.open_earn(
                        EarnPanel::Join {
                            peer: EarnPeer::User(me),
                        },
                        window,
                        cx,
                    );
                }))
                .into_any_element(),
        )
    }

    /// Per-frame: the balances behind the rows above, once per peer
    /// (tdesktop asks when the profile or the edit box opens).
    pub(super) fn earn_balances_upkeep(&mut self) {
        let Some(target) = self.session().and_then(|s| s.users_state.open_info_panel) else {
            return;
        };
        let ask = {
            let Some(session) = self.session() else {
                return;
            };
            match target {
                InfoPanelTarget::User(user_id) => session
                    .user(user_id)
                    .filter(|u| u.is_bot && u.can_be_edited)
                    .map(|_| (EarnPeer::User(user_id), true, true)),
                InfoPanelTarget::Supergroup(supergroup_id) => session
                    .supergroup_full_info(supergroup_id)
                    .map(|info| &info.admin)
                    .filter(|admin| {
                        admin.can_get_revenue_statistics || admin.can_get_star_revenue_statistics
                    })
                    .and_then(|admin| {
                        let chat = session.chats.values().find(|chat| {
                            matches!(
                                chat.kind,
                                ChatKind::Supergroup { supergroup_id: id, .. } if id == supergroup_id
                            )
                        })?;
                        Some((
                            EarnPeer::Chat(chat.id.0),
                            admin.can_get_revenue_statistics,
                            admin.can_get_star_revenue_statistics,
                        ))
                    }),
                _ => None,
            }
        };
        let Some(live) = self.live.as_mut() else {
            return;
        };
        // The affiliate rows' gates come from the app config.
        if matches!(
            target,
            InfoPanelTarget::User(_) | InfoPanelTarget::Supergroup(_)
        ) {
            let _ = live.driver.ensure_earn_app_config();
        }
        if let Some((peer, ton, stars)) = ask
            && !live
                .driver
                .session
                .payments
                .earn
                .balances_asked
                .contains(&peer)
        {
            let _ = live.driver.fetch_earn_balances(peer, ton, stars);
        }
    }
}

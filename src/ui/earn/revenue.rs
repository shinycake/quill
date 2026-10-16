//! The Monetization page (tdesktop `ChannelEarn::InnerWidget::fill`) and
//! the Stars Balance page (`BotEarn::InnerWidget::fill`): charts, the
//! rewards overview, the balances, the history and "Switch Off Ads".

use super::chart::EarnChartKind;
use super::*;
use crate::ui::chat_theme::{danger, success};
use crate::ui::premium_ui::star_tx_row;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::prelude::FluentBuilder;
use quill::earn::*;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::{StarAmount, TxFilter};
use quill::telegram::envelope::ChatKind;

/// tdesktop `FillLoading` with `LoadingType::Earn`.
fn loading() -> AnyElement {
    div()
        .id("earn-loading")
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .py_6()
        .child(Spinner::new())
        .child(
            div()
                .text_sm()
                .font_semibold()
                .child(tr(keys::STATS_EARN_LOADING)),
        )
        .child(about(tr(keys::STATS_EARN_LOADING_SUBTEXT)).text_center())
        .into_any_element()
}

/// A TON amount: the glyph, the whole part and the smaller fraction
/// (tdesktop `AddEmojiToMajor` + `MinorPart`).
pub(super) fn ton_amount(nanos: i64, major: f32, color: Hsla, sign: Option<bool>) -> Div {
    let prefix = match sign {
        Some(true) => "+",
        Some(false) => "\u{2212}",
        None => "",
    };
    div()
        .flex()
        .items_baseline()
        .text_color(color)
        .child(div().text_size(px(major)).child(prefix))
        .child(div().self_center().child(ton_icon(major * 0.8, color)))
        .child(
            div()
                .text_size(px(major))
                .font_semibold()
                .child(ton_major(nanos)),
        )
        .child(
            div()
                .text_size(px(major * 0.75))
                .font_semibold()
                .child(ton_minor(nanos)),
        )
}

/// A Stars amount with the gold star (tdesktop `CreateSingleStarWidget`).
pub(super) fn star_amount(amount: StarAmount, size: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(star_icon(size * 0.9))
        .child(
            div()
                .text_size(px(size))
                .font_semibold()
                .child(amount.label()),
        )
}

/// tdesktop `addAboutWithLearn`: the about text with its inline
/// "Learn more ›" link on the divider band.
pub(super) fn learn_more_band(
    id: &'static str,
    text: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    divider_label(about_with_link(id, text, on_click, cx), cx)
}

/// The about line with tdesktop's inline "Learn more ›" link.
pub(super) fn about_with_link(
    id: &'static str,
    text: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    about_link(id, text, LEARN_MORE, "", on_click, cx)
}

/// A click on a link that opens the learn box.
fn open_learn(cx: &mut Context<QuillApp>) -> impl Fn(&mut Window, &mut App) + 'static {
    let owner = cx.entity().downgrade();
    move |_, cx| {
        let _ = owner.update(cx, |this, cx| {
            this.earn.learn = true;
            cx.notify();
        });
    }
}

impl QuillApp {
    pub(super) fn revenue_page(
        &self,
        peer: EarnPeer,
        screen: EarnScreen,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return loading();
        };
        let Some(page) = session
            .payments
            .earn
            .page
            .as_ref()
            .filter(|p| p.peer == peer && p.screen == screen)
        else {
            return loading();
        };
        if !page.loaded() {
            return loading();
        }
        if screen == EarnScreen::Stars {
            return self.stars_screen(session, page, cx).into_any_element();
        }
        if page.ton.ready().is_none() && page.stars.ready().is_none() {
            let failed = match (&page.ton, &page.stars) {
                (Load::Failed(text), _) | (_, Load::Failed(text)) => text.clone(),
                _ => tr(keys::Q_MONEY_NO_RESULTS).to_string(),
            };
            return div()
                .id("earn-empty")
                .py_6()
                .flex()
                .justify_center()
                .text_sm()
                .font_semibold()
                .child(failed)
                .into_any_element();
        }
        self.monetization_screen(session, page, cx)
            .into_any_element()
    }

    fn monetization_screen(
        &self,
        session: &Session,
        page: &EarnPage,
        cx: &mut Context<Self>,
    ) -> Div {
        let peer = page.peer;
        let channel_id = match peer {
            EarnPeer::Chat(id) => Some(id),
            EarnPeer::User(_) => None,
        };
        let is_bot = channel_id.is_none();
        let ton = page.ton.ready();
        let stars = page.stars.ready().filter(|_| !is_bot);
        let mut body = div().flex().flex_col().gap_3();
        if ton.is_some() {
            body = body.child(learn_more_band(
                "earn-learn",
                if is_bot {
                    tr(keys::Q_MONEY_TELEGRAM_SHARES_50_OF_THE)
                } else {
                    tr(keys::Q_MONEY_TELEGRAM_SHARES_50_OF_THE_2)
                },
                open_learn(cx),
                cx,
            ));
        }
        // Charts: ad impressions, TON rewards, Stars revenue.
        let chart_chat = peer.chat_id();
        let mut charts = Vec::new();
        if let Some(ton) = ton {
            if let Some(graph) = ton.by_hour.as_ref() {
                charts.extend(self.earn_chart(
                    chart_chat,
                    "earn-chart-hours",
                    tr(keys::CHANNEL_EARN_CHART_TOP_HOURS),
                    graph,
                    EarnChartKind::Steps,
                    0.0,
                    cx,
                ));
            }
            if let Some(graph) = ton.revenue.as_ref() {
                charts.extend(self.earn_chart(
                    chart_chat,
                    "earn-chart-ton",
                    tr(keys::CHANNEL_EARN_CHART_REVENUE),
                    graph,
                    EarnChartKind::Columns,
                    ton.usd_rate * NANOS_PER_TON as f64,
                    cx,
                ));
            }
        }
        if let Some(graph) = stars.and_then(|s| s.by_day.as_ref()) {
            let rate = stars.map_or(0.0, |s| s.usd_rate);
            charts.extend(self.earn_chart(
                chart_chat,
                "earn-chart-stars",
                tr(keys::BOT_EARN_CHART_REVENUE),
                graph,
                EarnChartKind::Columns,
                rate,
                cx,
            ));
        }
        let has_charts = !charts.is_empty();
        for (ix, chart) in charts.into_iter().enumerate() {
            if ix > 0 {
                body = body.child(divider(cx));
            }
            body = body.child(chart);
        }
        if has_charts {
            body = body.child(divider(cx));
        }
        // Rewards overview: TON on the left, Stars on the right.
        let has_stars = stars.is_some_and(|s| s.status.has_any());
        let ton_rate = ton.map_or(0.0, |t| t.usd_rate);
        let star_rate = stars.map_or(0.0, |s| s.usd_rate);
        body = body.child(header(tr(keys::CHANNEL_EARN_OVERVIEW_TITLE), cx));
        let rows: [(&'static str, i64, StarAmount); 3] = [
            (
                tr(keys::CHANNEL_EARN_AVAILABLE),
                ton.map_or(0, |t| t.amounts.available),
                stars.map_or_else(StarAmount::default, |s| s.status.available),
            ),
            (
                tr(keys::CHANNEL_EARN_REWARD),
                ton.map_or(0, |t| t.amounts.balance),
                stars.map_or_else(StarAmount::default, |s| s.status.current),
            ),
            (
                tr(keys::CHANNEL_EARN_TOTAL),
                ton.map_or(0, |t| t.amounts.total),
                stars.map_or_else(StarAmount::default, |s| s.status.total),
            ),
        ];
        let fg = cx.theme().foreground;
        for (ix, (label, nanos, credits)) in rows.into_iter().enumerate() {
            let mut line = div().id(("earn-overview", ix)).flex().gap_3().flex_wrap();
            if ton.is_some() {
                line = line.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1()
                        .child(ton_amount(nanos, 16., fg, None))
                        .when(nanos != 0, |d| {
                            d.child(about(ton_to_usd(nanos, ton_rate, 3)))
                        }),
                );
            }
            if has_stars {
                line = line.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_1()
                        .child(star_amount(credits, 16.))
                        .when(credits != StarAmount::default(), |d| {
                            d.child(about(stars_to_usd(credits, star_rate, 0)))
                        }),
                );
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(line)
                    .child(about(label)),
            );
        }
        // Only the owner sees the balances and the history.
        if let Some(chat_id) = channel_id
            && !session.earn_is_channel_creator(chat_id)
        {
            return body;
        }
        body = body.child(divider(cx));
        if let Some(ton) = ton.filter(|t| t.amounts.available > 0) {
            body = body.child(self.ton_balance_block(peer, ton, cx));
        }
        if let Some(stars) = stars.filter(|s| s.status.available.stars > 0) {
            body = body
                .child(header(tr(keys::BOT_EARN_AVAILABLE), cx))
                .child(self.stars_withdrawal(session, page, stars, cx));
        }
        if session.earn_join_allowed(peer) {
            body = body
                .child(self.earn_stars_button(peer, cx))
                .child(divider(cx));
        }
        body = body.children(self.monetization_history(session, page, cx));
        if let Some(chat_id) = channel_id
            && session.chats.get(&chat_id).is_some_and(|c| {
                matches!(
                    c.kind,
                    ChatKind::Supergroup {
                        is_channel: true,
                        ..
                    }
                )
            })
        {
            body = body.child(self.switch_off_ads(session, chat_id, cx));
        }
        body
    }

    /// "Available reward": the TON to collect and the Collect button.
    fn ton_balance_block(
        &self,
        peer: EarnPeer,
        ton: &TonRevenueStats,
        cx: &mut Context<Self>,
    ) -> Div {
        let enabled = ton.amounts.withdrawal_enabled;
        let value = ton.amounts.available;
        let fg = cx.theme().foreground;
        let chat_id = peer.chat_id();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(header(tr(keys::CHANNEL_EARN_BALANCE_TITLE), cx))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .child(ton_amount(value, 26., fg, None)),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .child(about(ton_to_usd(value, ton.usd_rate, 0))),
            )
            .child(
                Button::new("earn-collect")
                    .debug_selector(|| "earn-collect".into())
                    .label(tr(keys::CHANNEL_EARN_BALANCE_BUTTON).to_uppercase())
                    .primary()
                    .w_full()
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.earn_withdraw(WithdrawTarget::Ton { chat_id }, cx);
                    })),
            )
            .child(learn_more_band(
                "earn-learn-collect",
                if enabled {
                    tr(keys::Q_MONEY_COLLECT_YOUR_REWARD_USING_FRAGMENT)
                } else {
                    tr(keys::Q_MONEY_IN_THE_COMING_WEEKS_YOU)
                },
                open_learn(cx),
                cx,
            ))
    }

    /// tdesktop `AddViewListButton` "Earn Stars".
    pub(super) fn earn_stars_button(&self, peer: EarnPeer, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("earn-stars-join")
            .flex()
            .gap_3()
            .p_2()
            .rounded_md()
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(tr(keys::CREDITS_SUMMARY_EARN_TITLE))
            .tab_index(0)
            .hover(|s| s.bg(cx.theme().secondary))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_earn(EarnPanel::Join { peer }, window, cx);
            }))
            .child(star_icon(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .child(tr(keys::CREDITS_SUMMARY_EARN_TITLE)),
                    )
                    .child(about(tr(keys::CREDITS_SUMMARY_EARN_ABOUT))),
            )
            .child(
                Icon::new(gpui_kit::assets::IconName::ChevronRight)
                    .size(px(16.))
                    .text_color(text_muted()),
            )
            .into_any_element()
    }

    /// The channel page's history: TON and Stars tabs, or one list.
    fn monetization_history(
        &self,
        session: &Session,
        page: &EarnPage,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let has_ton = !page.ton_history.rows.is_empty();
        let stars_list = page.history(TxFilter::All);
        let has_stars = matches!(page.peer, EarnPeer::Chat(_)) && !stars_list.rows.is_empty();
        if !has_ton && !has_stars {
            return None;
        }
        let show_stars = if has_ton && has_stars {
            page.stars_tab
        } else {
            has_stars
        };
        let mut block = div().flex().flex_col().gap_2();
        if has_ton && has_stars {
            let tab =
                |id: &'static str, label: &'static str, stars: bool, cx: &mut Context<Self>| {
                    let button = Button::new(id).label(label).small().on_click(cx.listener(
                        move |this, _, _, cx| {
                            if let Some(page) = this.earn_mut().and_then(|e| e.page.as_mut()) {
                                page.stars_tab = stars;
                            }
                            cx.notify();
                        },
                    ));
                    if show_stars == stars {
                        button.primary()
                    } else {
                        button.ghost()
                    }
                };
            block = block.child(
                div()
                    .flex()
                    .gap_1()
                    .child(tab(
                        "earn-tab-ton",
                        tr(keys::CHANNEL_EARN_CURRENCY_HISTORY),
                        false,
                        cx,
                    ))
                    .child(tab(
                        "earn-tab-stars",
                        tr(keys::CHANNEL_EARN_CREDITS_HISTORY),
                        true,
                        cx,
                    )),
            );
        } else if has_ton {
            block = block.child(header(tr(keys::CHANNEL_EARN_HISTORY_TITLE), cx));
        } else {
            block = block.child(header(tr(keys::CHANNEL_EARN_CREDITS_HISTORY), cx));
        }
        if show_stars {
            block = block.child(self.star_rows(session, page, TxFilter::All, cx));
        } else {
            block = block.child(self.ton_rows(page, cx));
        }
        Some(block.child(divider(cx)))
    }

    fn ton_rows(&self, page: &EarnPage, cx: &mut Context<Self>) -> Div {
        let mut list = div().flex().flex_col();
        for (ix, tx) in page.ton_history.rows.iter().enumerate() {
            let incoming = tx.is_incoming();
            let color: Hsla = if incoming { success() } else { danger() }.into();
            let date = tx.date_line();
            let failed = tx.failed();
            let row_tx = tx.clone();
            list = list.child(
                div()
                    .id(("earn-ton-tx", ix))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .py_1p5()
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "{}, {}",
                        tx.title(),
                        ton_signed_label(tx.amount, incoming)
                    ))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.earn.ton_tx = Some(row_tx.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().font_semibold().child(tx.title()))
                            .when(!date.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(if failed { danger() } else { text_muted() })
                                        .child(date),
                                )
                            }),
                    )
                    .child(ton_amount(tx.amount.abs(), 14., color, Some(incoming))),
            );
        }
        let history = &page.ton_history;
        if !history.next_offset.is_empty() {
            list = list.child(
                Button::new("earn-ton-more")
                    .label(if history.loading {
                        tr(keys::Q_MONEY_LOADING)
                    } else {
                        tr(keys::Q_MONEY_SHOW_MORE)
                    })
                    .ghost()
                    .disabled(history.loading)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.more_earn_ton_history();
                        }
                        cx.notify();
                    })),
            );
        }
        list
    }

    fn star_rows(
        &self,
        session: &Session,
        page: &EarnPage,
        filter: TxFilter,
        cx: &mut Context<Self>,
    ) -> Div {
        let history = page.history(filter);
        let mut list = div().flex().flex_col();
        for (ix, tx) in history.rows.iter().enumerate() {
            let row_tx = tx.clone();
            list = list.child(star_tx_row(
                session,
                ("earn-star-tx", ix + filter as usize * 100_000),
                tx,
                move |this, cx| {
                    this.earn.star_tx = Some(row_tx.clone());
                    cx.notify();
                },
                cx,
            ));
        }
        if !history.next_offset.is_empty() {
            list = list.child(
                Button::new(("earn-star-more", filter as usize))
                    .label(if history.loading {
                        tr(keys::Q_MONEY_LOADING)
                    } else {
                        tr(keys::Q_MONEY_SHOW_MORE)
                    })
                    .ghost()
                    .disabled(history.loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.more_earn_star_history(filter);
                        }
                        cx.notify();
                    })),
            );
        }
        list
    }

    /// tdesktop `BotEarn::InnerWidget::fill`: the bot's or the user's
    /// Stars.
    fn stars_screen(&self, session: &Session, page: &EarnPage, cx: &mut Context<Self>) -> Div {
        let Some(stats) = page.stars.ready() else {
            let text = match &page.stars {
                Load::Failed(text) => text.clone(),
                _ => tr(keys::Q_MONEY_NO_RESULTS).to_string(),
            };
            return div().py_6().text_sm().font_semibold().child(text);
        };
        let peer = page.peer;
        let is_self = matches!(peer, EarnPeer::User(id) if session.my_user_id == Some(id));
        let mut body = div().flex().flex_col().gap_3();
        if let Some(graph) = stats.by_day.as_ref()
            && let Some(chart) = self.earn_chart(
                peer.chat_id(),
                "earn-chart-bot",
                tr(keys::BOT_EARN_CHART_REVENUE),
                graph,
                EarnChartKind::Columns,
                stats.usd_rate,
                cx,
            )
        {
            body = body.child(chart).child(divider(cx));
        }
        body = body.child(header(tr(keys::BOT_EARN_OVERVIEW_TITLE), cx));
        let rows = [
            (tr(keys::BOT_EARN_AVAILABLE), stats.status.available),
            (tr(keys::BOT_EARN_REWARD), stats.status.current),
            (tr(keys::BOT_EARN_TOTAL), stats.status.total),
        ];
        for (ix, (label, amount)) in rows.into_iter().enumerate() {
            body = body.child(
                div()
                    .id(("earn-proceeds", ix))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_1()
                            .child(star_amount(amount, 16.))
                            .when(amount != StarAmount::default(), |d| {
                                d.child(about(stars_to_usd(amount, stats.usd_rate, 3)))
                            }),
                    )
                    .child(about(label)),
            );
        }
        body = body
            .child(divider_label(about(tr(keys::BOT_EARN_BALANCE_ABOUT)), cx))
            .child(header(tr(keys::BOT_EARN_AVAILABLE), cx))
            .child(self.stars_withdrawal(session, page, stats, cx));
        if !is_self && session.earn_join_allowed(peer) {
            body = body
                .child(self.earn_stars_button(peer, cx))
                .child(divider(cx));
        }
        if !is_self {
            body = body.children(self.stars_history(session, page, cx));
        }
        body
    }

    /// The Stars page's history: All, Incoming and Outgoing tabs, each
    /// shown only when it has rows.
    fn stars_history(
        &self,
        session: &Session,
        page: &EarnPage,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if page.history(TxFilter::All).rows.is_empty() {
            return None;
        }
        let has_in = !page.history(TxFilter::Incoming).rows.is_empty();
        let has_out = !page.history(TxFilter::Outgoing).rows.is_empty();
        let mut tabs = vec![TxFilter::All];
        if has_in {
            tabs.push(TxFilter::Incoming);
        }
        if has_out {
            tabs.push(TxFilter::Outgoing);
        }
        let active = if tabs.contains(&page.star_filter) {
            page.star_filter
        } else {
            TxFilter::All
        };
        let mut block = div().flex().flex_col().gap_2();
        if tabs.len() == 1 {
            block = block.child(header(tr(keys::Q_MONEY_ALL_TRANSACTIONS), cx));
        } else {
            let mut row = div().flex().gap_1();
            for filter in tabs {
                let label = match filter {
                    TxFilter::All => tr(keys::Q_MONEY_ALL_TRANSACTIONS),
                    TxFilter::Incoming => tr(keys::Q_MONEY_INCOMING),
                    TxFilter::Outgoing => tr(keys::Q_MONEY_OUTGOING),
                };
                let button = Button::new(("earn-star-tab", filter as usize))
                    .label(label)
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(page) = this.earn_mut().and_then(|e| e.page.as_mut()) {
                            page.star_filter = filter;
                        }
                        cx.notify();
                    }));
                row = row.child(if filter == active {
                    button.primary()
                } else {
                    button.ghost()
                });
            }
            block = block.child(row);
        }
        Some(block.child(self.star_rows(session, page, active, cx)))
    }

    /// "Switch Off Ads" with its boost-level lock (tdesktop
    /// `lng_channel_earn_off`).
    fn switch_off_ads(&self, session: &Session, chat_id: i64, cx: &mut Context<Self>) -> Div {
        let off = session
            .chat_supergroup(quill::ids::ChatId(chat_id))
            .and_then(|id| session.supergroup_full_info(id))
            .is_some_and(|info| !info.admin.can_have_sponsored_messages);
        let required = session
            .groups
            .chat_boost_features
            .get(&chat_id)
            .and_then(|f| {
                f.levels
                    .iter()
                    .find(|l| l.can_disable_sponsored_messages)
                    .map(|l| l.level)
            });
        let level = session.groups.chat_boost_status.get(&chat_id).map(|s| s.0);
        let locked = matches!((level, required), (Some(level), Some(required)) if level < required);
        let mut row = crate::ui::pressable::action_row(
            "earn-ads-off",
            None,
            tr(keys::CHANNEL_EARN_OFF),
            false,
            cx,
        )
        .aria_selected(off);
        if let Some(required) = required {
            row = row.child(
                div()
                    .px_1()
                    .rounded_sm()
                    .text_xs()
                    .font_semibold()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().primary_foreground)
                    .when(locked, |d| d.child("\u{1F512} "))
                    .child(tr_with(
                        keys::Q_MONEY_LEVEL,
                        &[("required", &required.to_string())],
                    )),
            );
        }
        row = row
            .child(div().flex_1())
            .child(Switch::new(("earn-ads-off", 1u64)).checked(off).small())
            .on_click(cx.listener(move |this, _, _, cx| {
                if locked && !off {
                    this.earn.ask_boost = Some((chat_id, required.unwrap_or(0)));
                    if let Some(live) = this.live.as_mut() {
                        let _ = live
                            .driver
                            .fetch_chat_boost_link(quill::ids::ChatId(chat_id));
                    }
                } else if let Some(live) = this.live.as_mut() {
                    let sent = live
                        .driver
                        .set_channel_sponsored_messages(quill::ids::ChatId(chat_id), off);
                    if sent.is_err() {
                        this.connection.status_note =
                            tr(keys::Q_MONEY_COULDNT_CHANGE_THE_ADS_SETTING).into();
                    }
                }
                cx.notify();
            }));
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(row)
            .child(divider_label(about(tr(keys::CHANNEL_EARN_OFF_ABOUT)), cx))
    }
}

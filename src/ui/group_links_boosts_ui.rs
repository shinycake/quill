//! The group and channel settings page for boosts (tdesktop's boosts
//! list and boost link of `info/boosts/`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::{civil_local, day_label, now_unix};
use quill::telegram::envelope::{ParsedBoostSource, ParsedChatBoost};

/// "boost expires on October 6" (tdesktop `lng_boosts_list_status`).
pub(crate) fn boost_status(boost: &ParsedChatBoost, now_unix_secs: i64) -> String {
    let at = civil_local(i64::from(boost.expiration_date));
    let now = civil_local(now_unix_secs);
    let expires = tr_with(
        keys::BOOSTS_LIST_STATUS,
        &[("date", &(day_label(&at, &now)).to_string())],
    );
    match boost.source {
        ParsedBoostSource::Premium { .. } => expires,
        ParsedBoostSource::GiftCode { .. } => tr_with(
            keys::Q_PEOPLE_GIFT_CODE,
            &[("expires", &expires.to_string())],
        ),
        ParsedBoostSource::Giveaway {
            is_unclaimed: true, ..
        } => tr(keys::BOOSTS_LIST_PENDING).into(),
        ParsedBoostSource::Giveaway { .. } => tr_with(
            keys::Q_PEOPLE_GIVEAWAY,
            &[("expires", &expires.to_string())],
        ),
    }
}

/// "12 boosts" / "1 boost" (tdesktop `lng_boosts_list_title`).
pub(crate) fn boost_count_title(count: i32) -> String {
    trn(keys::BOOSTS_LIST_TITLE, count)
}

/// "Show N more boosts" tail: "11 more boosts" / "1 more boost".
pub(crate) fn more_boosts_label(left: i32) -> String {
    let left = left.max(1);
    if left == 1 {
        tr(keys::Q_PEOPLE_1_MORE_BOOST).into()
    } else {
        tr_with(keys::Q_PEOPLE_MORE_BOOSTS, &[("left", &left.to_string())])
    }
}

impl QuillApp {
    fn is_channel_for_boosts(&self, chat_id: ChatId) -> bool {
        self.session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.kind.is_channel())
    }

    pub(super) fn open_boosts_page(&mut self, chat_id: ChatId, only_gifts: bool) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_chat_boosts(chat_id, only_gifts);
            let _ = live.driver.fetch_chat_boost_link(chat_id);
            let _ = live.driver.fetch_chat_boost_status(chat_id);
        }
        self.open_boost_extras(chat_id);
    }

    fn switch_boosts_tab(&mut self, chat_id: ChatId, only_gifts: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_chat_boosts(chat_id, only_gifts);
        }
        cx.notify();
    }

    /// Boosts list (All / Gifts) and the boost link.
    pub(super) fn settings_boosts(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let channel = self.is_channel_for_boosts(chat_id);
        let session = self.session();
        let list = session
            .and_then(|s| s.groups.chat_boost_lists.get(&chat_id.0))
            .cloned();
        let only_gifts = list.as_ref().is_some_and(|l| l.only_gifts);
        let level = session.and_then(|s| s.groups.chat_boost_status.get(&chat_id.0).copied());
        let mut body = div().flex().flex_col().gap_1().px_2();
        body = body.child(self.boosts_overview(chat_id, cx));
        body = body.child(self.giveaway_boosts_section(chat_id, cx));
        if let Some((level, count)) = level.filter(|_| {
            session.is_some_and(|s| !s.groups.chat_boost_details.contains_key(&chat_id.0))
        }) {
            body = body.child(div().text_sm().child(tr_with(
                keys::Q_PEOPLE_LEVEL,
                &[
                    ("level", &level.to_string()),
                    ("boost_count_title", &(boost_count_title(count)).to_string()),
                ],
            )));
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child({
                    let b = Button::new("boosts-tab-all")
                        .label(tr(keys::Q_PEOPLE_ALL_BOOSTS))
                        .small();
                    let b = if only_gifts { b.ghost() } else { b };
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.switch_boosts_tab(chat_id, false, cx);
                    }))
                })
                .child({
                    let b = Button::new("boosts-tab-gifts")
                        .label(tr(keys::Q_PEOPLE_GIFTS))
                        .small();
                    let b = if only_gifts { b } else { b.ghost() };
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.switch_boosts_tab(chat_id, true, cx);
                    }))
                }),
        );
        match &list {
            None => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_PEOPLE_LOADING)),
                );
            }
            Some(state) => {
                if !state.loading || !state.boosts.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(boost_count_title(state.total_count)),
                    );
                    if state.boosts.is_empty() {
                        body = body.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(tr(keys::Q_PEOPLE_NO_BOOSTS_YET)),
                        );
                    } else {
                        body = body.child(div().text_xs().text_color(muted).child(if channel {
                            tr(keys::BOOSTS_LIST_SUBTEXT)
                        } else {
                            tr(keys::BOOSTS_LIST_SUBTEXT_GROUP)
                        }));
                    }
                }
                let now = now_unix();
                for (index, boost) in state.boosts.iter().enumerate() {
                    let uid = boost.source.user_id();
                    let title = if uid == 0 {
                        tr(keys::BOOSTS_LIST_UNCLAIMED).to_string()
                    } else {
                        self.contact_display_name(uid)
                    };
                    // tdesktop opens the gift code box from a boost row that
                    // carries the code (`ResolveGiftCode`).
                    let code = boost.gift_code.0.clone();
                    let mut row = div()
                        .id(("boost-row", index as u64))
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_1()
                        .when(!code.is_empty(), |row| {
                            row.cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_gift_code_link(code.clone(), cx)
                                }))
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().truncate().child(title))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(boost_status(boost, now)),
                                ),
                        );
                    if boost.count > 1 {
                        row = row.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("×{}", boost.count)),
                        );
                    }
                    body = body.child(row);
                }
                if let Some(error) = state.error.clone() {
                    body = body.child(div().text_xs().text_color(muted).child(error));
                }
                if state.loading && !state.boosts.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_PEOPLE_LOADING)),
                    );
                } else if !state.next_offset.is_empty() && !state.loading {
                    let left = state.total_count - state.boosts.len() as i32;
                    body = body.child(
                        Button::new("boosts-more")
                            .label(tr_with(
                                keys::Q_PEOPLE_SHOW_2,
                                &[("more_boosts_label", &(more_boosts_label(left)).to_string())],
                            ))
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.load_more_chat_boosts(chat_id);
                                }
                                cx.notify();
                            })),
                    );
                }
            }
        }
        if let Some((link, _)) = session.and_then(|s| s.groups.chat_boost_links.get(&chat_id.0)) {
            let copy = link.clone();
            body = body
                .child(
                    div()
                        .pt_2()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::BOOSTS_LINK_TITLE)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .child(link.clone()),
                        )
                        .child(
                            Button::new("boost-link-copy")
                                .label(tr(keys::GROUP_INVITE_CONTEXT_COPY))
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                    this.connection.set_note(StatusNote::info(tr(
                                        keys::Q_PEOPLE_BOOST_LINK_COPIED,
                                    )));
                                    cx.notify();
                                })),
                        ),
                )
                .child(div().text_xs().text_color(muted).child(if channel {
                    tr(keys::BOOSTS_LINK_SUBTEXT)
                } else {
                    tr(keys::BOOSTS_LINK_SUBTEXT_GROUP)
                }));
        }
        body.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{boost_count_title, boost_status};
    use quill::telegram::envelope::{ParsedBoostSource, ParsedChatBoost};

    fn boost(source: ParsedBoostSource) -> ParsedChatBoost {
        ParsedChatBoost {
            id: "b".into(),
            count: 1,
            source,
            start_date: 1,
            expiration_date: 1_800_000_000,
            gift_code: Default::default(),
        }
    }

    #[test]
    fn boost_status_by_source() {
        let now = 1_790_000_000;
        assert!(
            boost_status(&boost(ParsedBoostSource::Premium { user_id: 1 }), now)
                .starts_with("boost expires on ")
        );
        assert!(
            boost_status(&boost(ParsedBoostSource::GiftCode { user_id: 1 }), now)
                .starts_with("gift code · boost expires on ")
        );
        assert_eq!(
            boost_status(
                &boost(ParsedBoostSource::Giveaway {
                    user_id: 0,
                    is_unclaimed: true
                }),
                now
            ),
            "To be distributed"
        );
    }

    #[test]
    fn more_label_pluralises() {
        assert_eq!(super::more_boosts_label(11), "11 more boosts");
        assert_eq!(super::more_boosts_label(1), "1 more boost");
        assert_eq!(super::more_boosts_label(0), "1 more boost");
    }

    #[test]
    fn boost_titles_pluralise() {
        assert_eq!(boost_count_title(1), "1 boost");
        assert_eq!(boost_count_title(0), "0 boosts");
        assert_eq!(boost_count_title(12), "12 boosts");
    }
}

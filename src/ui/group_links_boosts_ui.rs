//! Group and channel settings pages for username order and boosts
//! (tdesktop `edit_peer_usernames_list.cpp` "Link order" and the boosts
//! list and boost link of `info/boosts/`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_time::{civil_local, day_label, now_unix};
use quill::telegram::envelope::{ParsedBoostSource, ParsedChatBoost, SupergroupUsernames};

/// Status under a username row.
pub(crate) fn username_status(lists: &SupergroupUsernames, name: &str) -> &'static str {
    if lists.active.iter().any(|n| n == name) {
        if name == lists.editable {
            tr(keys::Q_PEOPLE_MAIN_LINK)
        } else {
            "active"
        }
    } else {
        "inactive"
    }
}

/// Usernames in display order: active ones in their order, then the
/// disabled ones.
pub(crate) fn username_rows(lists: &SupergroupUsernames) -> Vec<String> {
    lists
        .active
        .iter()
        .chain(lists.disabled.iter())
        .cloned()
        .collect()
}

/// Confirmation text for showing or hiding a link (tdesktop
/// `lng_channel_usernames_*_description`).
pub(crate) fn username_confirm_text(channel: bool, show: bool) -> String {
    let place = if channel { "channel" } else { "group" };
    if show {
        tr_with(keys::Q_PEOPLE_SHOW_THIS_LINK_ON_THE, &[("place", place)])
    } else {
        tr_with(keys::Q_PEOPLE_HIDE_THIS_LINK_FROM_THE, &[("place", place)])
    }
}

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

    fn change_username(
        &mut self,
        chat_id: ChatId,
        name: &str,
        action: UsernameChange,
        cx: &mut Context<Self>,
    ) {
        self.connection.set_note(match self.live.as_mut() {
            Some(live) => {
                let sent = match action {
                    UsernameChange::Toggle(on) => {
                        live.driver.toggle_group_username(chat_id, name, on)
                    }
                    UsernameChange::Move(up) => live.driver.move_group_username(chat_id, name, up),
                };
                match sent {
                    Ok(Some(_)) => StatusNote::default(),
                    Ok(None) => StatusNote::error(tr(keys::Q_PEOPLE_THAT_LINK_CANT_BE_CHANGED)),
                    Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY)),
                }
            }
            None => StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)),
        });
        cx.notify();
    }

    fn confirm_username_toggle(
        &mut self,
        chat_id: ChatId,
        name: String,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let channel = self.is_channel_for_boosts(chat_id);
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            let name = name.clone();
            alert
                .title(format!(
                    "{} @{name}?",
                    if show {
                        tr(keys::Q_PEOPLE_SHOW)
                    } else {
                        tr(keys::Q_PEOPLE_HIDE)
                    }
                ))
                .description(username_confirm_text(channel, show))
                .ok_text(if show {
                    tr(keys::Q_PEOPLE_SHOW)
                } else {
                    tr(keys::Q_PEOPLE_HIDE)
                })
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.change_username(chat_id, &name, UsernameChange::Toggle(show), cx);
                    });
                    true
                })
        });
    }

    /// "Link order": the group's usernames, reorderable and toggleable.
    pub(super) fn settings_usernames(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(lists) = self
            .session()
            .and_then(|s| s.chat_usernames(chat_id))
            .cloned()
        else {
            return div()
                .px_2()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_PEOPLE_NO_LINKS_TO_ARRANGE))
                .into_any_element();
        };
        let rows = username_rows(&lists);
        let active_count = lists.active.len();
        let mut body = div().flex().flex_col().gap_1();
        for (index, name) in rows.iter().enumerate() {
            let active = index < active_count;
            let status = username_status(&lists, name);
            let up = name.clone();
            let down = name.clone();
            let toggle = name.clone();
            let mut actions = div().flex().items_center().gap_1();
            if active && index > 0 {
                actions = actions.child(
                    Button::new(("username-up", index as u64))
                        .icon(IconName::ArrowUp)
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::Q_PEOPLE_MOVE_UP))
                        .accessibility_label(tr(keys::Q_PEOPLE_MOVE_UP))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_username(chat_id, &up, UsernameChange::Move(true), cx);
                        })),
                );
            }
            if active && index + 1 < active_count {
                actions = actions.child(
                    Button::new(("username-down", index as u64))
                        .icon(IconName::ArrowDown)
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::Q_PEOPLE_MOVE_DOWN))
                        .accessibility_label(tr(keys::Q_PEOPLE_MOVE_DOWN))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_username(chat_id, &down, UsernameChange::Move(false), cx);
                        })),
                );
            }
            // The main link can't be turned off.
            if *name != lists.editable {
                actions = actions.child(
                    Button::new(("username-toggle", index as u64))
                        .label(if active {
                            tr(keys::Q_PEOPLE_HIDE)
                        } else {
                            tr(keys::Q_PEOPLE_SHOW)
                        })
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_username_toggle(
                                chat_id,
                                toggle.clone(),
                                !active,
                                window,
                                cx,
                            );
                        })),
                );
            }
            body = body.child(
                div()
                    .id(("username-row", index as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .px_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .text_color(if active { cx.theme().foreground } else { muted })
                                    .child(format!("@{name}")),
                            )
                            .child(div().text_xs().text_color(muted).child(status)),
                    )
                    .child(actions),
            );
        }
        body.child(
            div()
                .px_2()
                .pt_1()
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_PEOPLE_LINKS_ARE_SHOWN_ON_THE)),
        )
        .into_any_element()
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

#[derive(Clone, Copy)]
enum UsernameChange {
    Toggle(bool),
    Move(bool),
}

#[cfg(test)]
mod tests {
    use super::{
        boost_count_title, boost_status, username_confirm_text, username_rows, username_status,
    };
    use quill::telegram::envelope::{ParsedBoostSource, ParsedChatBoost, SupergroupUsernames};

    fn lists() -> SupergroupUsernames {
        SupergroupUsernames {
            active: vec!["main".into(), "club".into()],
            disabled: vec!["old".into()],
            editable: "main".into(),
            collectible: vec!["club".into(), "old".into()],
        }
    }

    #[test]
    fn rows_list_active_first_with_status() {
        let lists = lists();
        assert_eq!(username_rows(&lists), vec!["main", "club", "old"]);
        assert_eq!(username_status(&lists, "main"), "main link");
        assert_eq!(username_status(&lists, "club"), "active");
        assert_eq!(username_status(&lists, "old"), "inactive");
    }

    #[test]
    fn confirm_text_names_the_place() {
        assert_eq!(
            username_confirm_text(true, true),
            "Show this link on the channel info page?"
        );
        assert_eq!(
            username_confirm_text(false, false),
            "Hide this link from the group info page?"
        );
    }

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

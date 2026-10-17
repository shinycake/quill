//! B8: invite-link admin blocks under the info panel's "Invite links"
//! section — who joined through a link, other admins' link counts, and
//! the revoked-links list with delete (tdesktop `edit_peer_invite_links.cpp`
//! and `edit_peer_invite_link.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{civil_local, day_label, now_unix};
use quill::state::{InviteLinkCountsFetch, InviteLinkFetch};
use quill::telegram::envelope::StarSubscriptionPricing;

/// "250 Stars / month" for a subscription link's pricing.
pub(crate) fn subscription_price_label(pricing: &StarSubscriptionPricing) -> String {
    tr_with(
        keys::Q_OPS_STARS_MONTH,
        &[("star_count", &pricing.star_count.to_string())],
    )
}

/// The "Loading..." line of the invite-link blocks, with the kit spinner.
pub(crate) fn loading_row(color: Hsla) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(color)
        .child(gpui_kit::component::spinner::Spinner::new().xsmall())
        .child(tr(keys::Q_OPS_LOADING))
}

/// "3 invite links" / "1 invite link", with revoked links appended
/// (tdesktop `lng_group_invite_other_count`).
pub(crate) fn invite_link_count_label(active: i32, revoked: i32) -> String {
    let base = if active == 1 {
        tr(keys::Q_OPS_1_INVITE_LINK).to_string()
    } else {
        tr_with(keys::Q_OPS_INVITE_LINKS, &[("active", &active.to_string())])
    };
    if revoked > 0 {
        tr_with(
            keys::Q_OPS_REVOKED,
            &[
                ("base", &base.to_string()),
                ("revoked", &revoked.to_string()),
            ],
        )
    } else {
        base
    }
}

/// "joined today" / "joined 12 Sep" for a member row.
pub(crate) fn joined_label(date: i32, via_folder: bool) -> String {
    if via_folder {
        return tr(keys::GROUP_INVITE_JOINED_VIA_FILTER).into();
    }
    if date <= 0 {
        return "joined".into();
    }
    let at = civil_local(i64::from(date));
    let now = civil_local(now_unix());
    format!("joined {}", day_label(&at, &now))
}

impl QuillApp {
    pub(super) fn invite_block_title(&self, text: &str, cx: &mut Context<Self>) -> Div {
        div()
            .text_xs()
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child(text.to_owned())
    }

    /// Toggle the "who joined" details of one link.
    pub(super) fn toggle_invite_link_details(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .admin
            .invite_link_details
            .as_ref()
            .is_some_and(|(chat, link)| *chat == chat_id && link == invite_link);
        if open {
            self.admin.invite_link_details = None;
            if let Some(live) = self.live.as_mut() {
                live.driver.close_chat_invite_link_members(chat_id);
            }
        } else {
            self.admin.invite_link_details = Some((chat_id, invite_link.to_owned()));
            if let Some(live) = self.live.as_mut() {
                let _ = live
                    .driver
                    .open_chat_invite_link_members(chat_id, invite_link);
            }
        }
        cx.notify();
    }

    /// Show or hide the revoked-links list (fetched on first open).
    pub(super) fn toggle_revoked_links(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.admin.revoked_links_open = !self.admin.revoked_links_open;
        if self.admin.revoked_links_open
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_revoked_chat_invite_links(chat_id);
        }
        cx.notify();
    }

    fn delete_revoked_link(&mut self, chat_id: ChatId, invite_link: &str, cx: &mut Context<Self>) {
        self.connection.set_note(match self.live.as_mut() {
            Some(live) => match live
                .driver
                .delete_revoked_chat_invite_link(chat_id, invite_link)
            {
                Ok(_) => StatusNote::info(tr(keys::Q_OPS_DELETING_INVITE_LINK)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_DELETE_INVITE_LINK)),
            },
            None => StatusNote::info(tr(keys::Q_OPS_INVITE_LINKS_NEED_A_LIVE)),
        });
        cx.notify();
    }

    fn confirm_delete_all_revoked(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title(tr(keys::Q_OPS_DELETE_ALL_REVOKED_LINKS))
                .description(tr(keys::GROUP_INVITE_DELETE_ALL_SURE))
                .ok_text(tr(keys::GROUP_INVITE_CONTEXT_DELETE))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.connection.set_note(match this.live.as_mut() {
                            Some(live) => {
                                match live.driver.delete_all_revoked_chat_invite_links(chat_id) {
                                    Ok(_) => {
                                        StatusNote::info(tr(keys::Q_OPS_DELETING_REVOKED_LINKS))
                                    }
                                    Err(_) => StatusNote::error(tr(
                                        keys::Q_OPS_COULD_NOT_DELETE_REVOKED_LINKS,
                                    )),
                                }
                            }
                            None => StatusNote::info(tr(keys::Q_OPS_INVITE_LINKS_NEED_A_LIVE)),
                        });
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn load_more_link_members(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_chat_invite_link_members(chat_id);
        }
        cx.notify();
    }

    /// Members who joined through `invite_link` (paged), shown under the
    /// link's row while its details are open.
    pub(super) fn invite_link_members_block(
        &self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self
            .admin
            .invite_link_details
            .as_ref()
            .is_some_and(|(chat, link)| *chat == chat_id && link == invite_link);
        if !open {
            return None;
        }
        let state = self
            .session()
            .and_then(|s| s.groups.invite_link_members.get(&chat_id.0))
            .filter(|state| state.invite_link == invite_link)
            .cloned();
        let muted = cx.theme().muted_foreground;
        let mut block = div()
            .id(format!("invite-link-members-{}", invite_link.len()))
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .pl_2();
        let Some(state) = state else {
            return Some(
                block
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_OPS_LOADING)),
                    )
                    .into_any_element(),
            );
        };
        block = block.child(self.invite_block_title(
            &if state.total_count == 0 && !state.loading {
                tr(keys::GROUP_INVITE_NO_JOINED).to_string()
            } else if state.total_count == 1 {
                tr(keys::Q_OPS_1_JOINED).to_string()
            } else {
                format!("{} joined", state.total_count)
            },
            cx,
        ));
        for member in &state.members {
            let uid = member.user_id;
            let name = self.contact_display_name(uid);
            block = block.child(
                div()
                    .id(("invite-link-member", uid as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(name))
                    .child(div().text_xs().text_color(muted).child(joined_label(
                        member.joined_chat_date,
                        member.via_chat_folder_invite_link,
                    ))),
            );
        }
        if let Some(error) = state.error.clone() {
            block = block.child(div().text_xs().text_color(muted).child(error));
        }
        if state.loading {
            block = block.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_LOADING)),
            );
        } else if (state.members.len() as i32) < state.total_count {
            block = block.child(
                Button::new("invite-link-members-more")
                    .label(tr(keys::CHANNELS_YOUR_MORE))
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.load_more_link_members(chat_id, cx);
                    })),
            );
        }
        Some(block.into_any_element())
    }

    /// Per-admin link counts (owner only; the driver gates the fetch).
    pub(super) fn invite_link_counts_block(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(open) = self.admin_links_block(chat_id, cx) {
            return Some(open);
        }
        let Some(InviteLinkCountsFetch::Loaded(counts)) = self
            .session()
            .and_then(|s| s.groups.invite_link_counts.get(&chat_id.0))
            .cloned()
        else {
            return None;
        };
        let me = self.session().and_then(|s| s.my_user_id);
        let others: Vec<_> = counts
            .into_iter()
            .filter(|count| Some(count.user_id) != me)
            .collect();
        if others.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut block = div().flex().flex_col().w_full().gap_1().pt_1();
        block = block.child(self.invite_block_title(tr(keys::GROUP_INVITE_OTHER_TITLE), cx));
        for count in others {
            let uid = count.user_id;
            block = block.child(
                div()
                    .id(("invite-link-count", uid as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(self.contact_display_name(uid)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(invite_link_count_label(
                                count.invite_link_count,
                                count.revoked_invite_link_count,
                            )),
                    )
                    .child(
                        Button::new(("invite-link-count-open", uid as u64))
                            .label(tr(keys::Q_OPS_VIEW))
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_admin_links(chat_id, uid, cx);
                            })),
                    ),
            );
        }
        Some(block.into_any_element())
    }

    /// "Revoked links" toggle plus, when open, the list with per-link
    /// Delete and "Delete all".
    pub(super) fn revoked_invite_links_block(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let open = self.admin.revoked_links_open;
        let mut block = div().flex().flex_col().w_full().gap_1().pt_1().child(
            div().flex().items_center().w_full().gap_1().child(
                // A disclosure row: the section title with a chevron
                // that turns when the list is open.
                Button::new("revoked-links-toggle")
                    .icon(if open {
                        gpui_kit::assets::IconName::ChevronDown
                    } else {
                        gpui_kit::assets::IconName::ChevronRight
                    })
                    .label(tr(keys::GROUP_INVITE_REVOKED_TITLE))
                    .ghost()
                    .small()
                    .tooltip(if open {
                        tr(keys::Q_OPS_HIDE)
                    } else {
                        tr(keys::Q_OPS_SHOW)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_revoked_links(chat_id, cx);
                    })),
            ),
        );
        if !open {
            return block.into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|s| s.groups.revoked_invite_links.get(&chat_id.0))
            .cloned();
        match fetch {
            None | Some(InviteLinkFetch::Loading) => {
                block = block.child(loading_row(muted));
            }
            Some(InviteLinkFetch::Failed(message)) => {
                block = block.child(div().text_xs().text_color(muted).child(message));
            }
            Some(InviteLinkFetch::Loaded(list)) => {
                if list.links.is_empty() {
                    block = block.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_OPS_NO_REVOKED_LINKS)),
                    );
                } else {
                    block = block.child(
                        Button::new("revoked-links-delete-all")
                            .label(tr(keys::Q_OPS_DELETE_ALL_REVOKED_LINKS))
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_delete_all_revoked(chat_id, window, cx);
                            })),
                    );
                }
                for (index, link) in list.links.into_iter().enumerate() {
                    let delete_link = link.invite_link.clone();
                    let name = if link.name.is_empty() {
                        link.invite_link.clone()
                    } else {
                        link.name.clone()
                    };
                    block = block.child(
                        div()
                            .id(("revoked-link-row", index as u64))
                            .flex()
                            .items_center()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_sm().truncate().child(name))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(format!("{} uses", link.member_count)),
                                    ),
                            )
                            .child(
                                Button::new(format!("revoked-link-delete-{index}"))
                                    .label(tr(keys::GROUP_INVITE_CONTEXT_DELETE))
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_revoked_link(chat_id, &delete_link, cx);
                                    })),
                            ),
                    );
                }
            }
        }
        block.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{invite_link_count_label, joined_label, subscription_price_label};
    use quill::telegram::envelope::StarSubscriptionPricing;

    #[test]
    fn count_labels_pluralise_and_append_revoked() {
        assert_eq!(invite_link_count_label(1, 0), "1 invite link");
        assert_eq!(invite_link_count_label(3, 0), "3 invite links");
        assert_eq!(invite_link_count_label(2, 4), "2 invite links, 4 revoked");
    }

    #[test]
    fn joined_label_handles_folder_and_missing_dates() {
        assert_eq!(joined_label(0, false), "joined");
        assert_eq!(joined_label(5, true), "joined via a folder invite link");
        assert!(joined_label(1_700_000_000, false).starts_with("joined "));
    }

    #[test]
    fn price_label_reads_stars_per_month() {
        let pricing = StarSubscriptionPricing {
            period: 2_592_000,
            star_count: 250,
        };
        assert_eq!(subscription_price_label(&pricing), "250 Stars / month");
    }
}

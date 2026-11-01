//! The Type box's sections, top to bottom as tdesktop lays them out:
//! the Public / Private choice, the link or the primary invite link, the
//! usernames, "Who can send messages?" and content protection.
#![deny(clippy::indexing_slicing)]

use super::{GroupTypeBox, TypeBoxFacts};
use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::group_type::{LinkStatus, join_approval_applies, request_to_join_about};
use quill::lang::{keys, tr, trn};

pub(super) fn heading(text: &str, cx: &App) -> Div {
    div()
        .pt_2()
        .text_sm()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(text.to_string())
}

pub(super) fn about(text: &str, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}

/// A labeled on/off setting: the whole row toggles it.
fn switch_row(
    id: &'static str,
    label: &'static str,
    on: bool,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Switch::new(id)
        .label(label)
        .checked(on)
        .on_change(on_change)
}

impl QuillApp {
    pub(in crate::ui) fn group_type_box_body(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let facts = self.group_type_facts()?;
        let type_box = self.group_type_box()?;
        let public = type_box.draft.public && !facts.public_blocked;
        let mut body = div()
            .id("group-type-box")
            .flex()
            .flex_col()
            .gap_3()
            .child(self.group_type_choice(&facts, public, cx));
        if facts.setup {
            if public {
                body = body.child(self.group_type_link(type_box, &facts, cx));
            } else if !facts.group {
                body = body.child(self.new_chat_invite_link(facts.chat_id, cx));
            }
            return Some(body.into_any_element());
        }
        if public {
            body = body.child(self.group_type_link(type_box, &facts, cx));
            if let Some(list) = self.group_type_usernames(type_box, &facts, cx) {
                body = body.child(list);
            }
        } else {
            body = body.child(self.group_type_invite_link(&facts, cx));
        }
        if let Some(send) = group_type_who_can_send(type_box, &facts, public, cx) {
            body = body.child(send);
        }
        body = body.child(group_type_no_forwards(type_box, &facts, cx));
        Some(body.into_any_element())
    }

    fn group_type_choice(
        &self,
        facts: &TypeBoxFacts,
        public: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (public_title, public_about, private_title, private_about) = if facts.group {
            (
                keys::CREATE_PUBLIC_GROUP_TITLE,
                keys::CREATE_PUBLIC_GROUP_ABOUT,
                keys::CREATE_PRIVATE_GROUP_TITLE,
                keys::CREATE_PRIVATE_GROUP_ABOUT,
            )
        } else {
            (
                keys::CREATE_PUBLIC_CHANNEL_TITLE,
                keys::CREATE_PUBLIC_CHANNEL_ABOUT,
                keys::CREATE_PRIVATE_CHANNEL_TITLE,
                keys::CREATE_PRIVATE_CHANNEL_ABOUT,
            )
        };
        // The kit's radio label runs past the dialog edge when it wraps.
        let option = |id: &'static str, title, text, on: bool, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Radio::new(id)
                        .label(tr(title))
                        .checked(on)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_group_public(id == "group-type-public", cx);
                        })),
                )
                .child(about(tr(text), cx).pl_6())
        };
        div()
            .id("group-type-choice")
            .flex()
            .flex_col()
            .gap_3()
            .child(option(
                "group-type-public",
                public_title,
                public_about,
                public,
                cx,
            ))
            .child(option(
                "group-type-private",
                private_title,
                private_about,
                !public,
                cx,
            ))
            .into_any_element()
    }

    /// The t.me link field and tdesktop's line under it.
    fn group_type_link(
        &self,
        type_box: &GroupTypeBox,
        facts: &TypeBoxFacts,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = type_box.status(self, facts, cx);
        let theme = cx.theme();
        let (danger, muted, good) = (theme.danger, theme.muted_foreground, theme.success);
        let line: Option<AnyElement> = match status {
            LinkStatus::Empty | LinkStatus::Current => None,
            LinkStatus::Pending => Some(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::CREATE_CHANNEL_LINK_PENDING))
                    .into_any_element(),
            ),
            LinkStatus::Available => Some(
                div()
                    .id("group-type-link-status")
                    .text_xs()
                    .text_color(good)
                    .child(tr(keys::CREATE_CHANNEL_LINK_AVAILABLE))
                    .into_any_element(),
            ),
            LinkStatus::Problem(problem) => Some(
                div()
                    .id("group-type-link-status")
                    .text_xs()
                    .text_color(danger)
                    .child(tr(problem.text()))
                    .into_any_element(),
            ),
            LinkStatus::Purchasable => {
                let app = cx.weak_entity();
                Some(self.group_type_purchase_line(type_box, app, cx))
            }
        };
        let failed = if facts.setup {
            self.new_chat_username_failure()
        } else {
            self.group_edit_box()
                .and_then(|edit| edit.username_failure(self.session()))
                .filter(|_| status == LinkStatus::Current)
        }
        .map(|reason| {
            div()
                .text_xs()
                .text_color(danger)
                .child(reason)
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading(tr(keys::CREATE_GROUP_LINK), cx))
            .child(
                Input::new(&type_box.link_input)
                    .aria_label(tr(keys::CREATE_GROUP_LINK))
                    .prefix(div().text_color(muted).child("t.me/")),
            )
            .children(line)
            .children(failed)
            .when(!facts.setup, |block| {
                block.child(about(tr(keys::CREATE_CHANNEL_LINK_ABOUT), cx))
            })
            .into_any_element()
    }

    /// "Sorry, this link is taken. But it's available for purchase.
    /// Learn more..." with the Fragment address (tdesktop
    /// `UsernameCheckInfo::PurchaseAvailable`).
    fn group_type_purchase_line(
        &self,
        type_box: &GroupTypeBox,
        app: WeakEntity<QuillApp>,
        cx: &App,
    ) -> AnyElement {
        let name = type_box.link_text(cx);
        let url = self
            .session()
            .and_then(|s| s.payments.hub.fragment.username_url(&name));
        let text = tr(keys::USERNAME_PURCHASE_AVAILABLE);
        let (before, after) = text.split_once("{link}").unwrap_or((text, ""));
        let danger = cx.theme().danger;
        match url {
            Some(url) => div()
                .id("group-type-link-status")
                .text_xs()
                .text_color(danger)
                .child(crate::ui::fragment_link::text_with_link(
                    "group-type-link-fragment",
                    before,
                    tr(keys::USERNAME_PURCHASE_AVAILABLE_LINK),
                    after,
                    url,
                    app,
                    cx,
                ))
                .into_any_element(),
            None => div()
                .id("group-type-link-status")
                .text_xs()
                .text_color(danger)
                .child(tr(keys::CREATE_CHANNEL_LINK_OCCUPIED))
                .into_any_element(),
        }
    }

    /// A private chat's primary invite link (tdesktop
    /// `AddPermanentLinkBlock`): the link, its menu, Copy and Share, who
    /// joined.
    fn group_type_invite_link(&self, facts: &TypeBoxFacts, cx: &mut Context<Self>) -> AnyElement {
        let link = self
            .session()
            .and_then(|s| s.chat_primary_invite_link(facts.chat_id))
            .cloned();
        let muted = cx.theme().muted_foreground;
        let mut block = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading(tr(keys::CREATE_PERMANENT_LINK_TITLE), cx));
        let Some(link) = link else {
            return block
                .child(about(tr(keys::Q_PEOPLE_LOADING), cx))
                .into_any_element();
        };
        let shown = link.invite_link.trim_start_matches("https://").to_string();
        let owner = cx.entity().downgrade();
        let chat_id = facts.chat_id;
        let menu_link = link.invite_link.clone();
        let menu = Button::new("group-type-link-menu")
            .icon(IconName::Ellipsis)
            .ghost()
            .small()
            .accessibility_label(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .dropdown_menu(move |menu, _, _| {
                let item = |label: &'static str, action: fn(&mut QuillApp, ChatIdLink, &mut Context<QuillApp>)| {
                    let owner = owner.clone();
                    let target = ChatIdLink(chat_id, menu_link.clone());
                    PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let target = target.clone();
                        let _ = owner.update(cx, |this, cx| action(this, target, cx));
                    })
                };
                menu.item(item(tr(keys::GROUP_INVITE_CONTEXT_COPY), |this, t, cx| {
                    this.copy_invite_link(&t.1, cx)
                }))
                .item(item(tr(keys::GROUP_INVITE_CONTEXT_SHARE), |this, t, cx| {
                    this.share_group_type_link(t.1, cx)
                }))
                .item(item(tr(keys::GROUP_INVITE_CONTEXT_QR), |this, t, cx| {
                    this.open_invite_link_qr(&t.1, cx)
                }))
                .item(item(tr(keys::GROUP_INVITE_CONTEXT_REVOKE), |this, t, cx| {
                    this.revoke_invite_link(t.0, &t.1, cx)
                }))
            });
        let copy_link = link.invite_link.clone();
        let share_link = link.invite_link.clone();
        let joined = if link.member_count > 0 {
            trn(keys::GROUP_INVITE_JOINED, link.member_count)
        } else {
            tr(keys::GROUP_INVITE_NO_JOINED).to_string()
        };
        block = block
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .child(div().flex_1().min_w_0().truncate().text_sm().child(shown))
                    .child(menu),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("group-type-copy-link")
                            .label(tr(keys::GROUP_INVITE_COPY))
                            .primary()
                            .flex_1()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_invite_link(&copy_link, cx);
                            })),
                    )
                    .child(
                        Button::new("group-type-share-link")
                            .label(tr(keys::GROUP_INVITE_SHARE))
                            .primary()
                            .flex_1()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.share_group_type_link(share_link.clone(), cx);
                            })),
                    ),
            )
            .child(div().text_xs().text_color(muted).child(joined))
            .child(about(
                tr(if facts.group {
                    keys::GROUP_INVITE_ABOUT_PERMANENT_GROUP
                } else {
                    keys::GROUP_INVITE_ABOUT_PERMANENT_CHANNEL
                }),
                cx,
            ));
        block.into_any_element()
    }

    /// Share Link: the chat chooser with the link as the message.
    fn share_group_type_link(&mut self, link: String, cx: &mut Context<Self>) {
        self.share.link_text = Some(link);
        cx.notify();
    }

    /// Copy one of the usernames' t.me links.
    pub(super) fn copy_group_type_username(&mut self, name: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(format!("https://t.me/{name}")));
        self.connection
            .set_note(StatusNote::info(tr(keys::CREATE_CHANNEL_LINK_COPIED)));
        cx.notify();
    }
}

/// A chat and one of its invite links, for the link menu's actions.
#[derive(Clone)]
struct ChatIdLink(quill::ids::ChatId, String);

/// "Who can send messages?" (tdesktop `fillWhoCanSend` area of
/// `EditPeerTypeBox`): supergroups and channels only; a public broadcast
/// channel has nothing to approve.
fn group_type_who_can_send(
    type_box: &GroupTypeBox,
    facts: &TypeBoxFacts,
    public: bool,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    if facts.basic || !join_approval_applies(public, facts.group) {
        return None;
    }
    let draft = &type_box.draft;
    let mut section = div().flex().flex_col().gap_2();
    if facts.discussion {
        section = section
            .child(heading(tr(keys::MANAGE_PEER_SEND_TITLE), cx))
            .child(switch_row(
                "group-type-only-members",
                tr(keys::MANAGE_PEER_SEND_ONLY_MEMBERS),
                draft.join_to_write,
                cx.listener(|this, &on, _, cx| {
                    this.edit_group_type_draft(cx, |draft| draft.join_to_write = on);
                }),
            ));
    }
    let approve_shown = !facts.discussion || draft.join_to_write;
    if approve_shown {
        section = section.child(switch_row(
            "group-type-approve",
            tr(if facts.group {
                keys::MANAGE_PEER_SEND_APPROVE_MEMBERS
            } else {
                keys::MANAGE_PEER_SEND_APPROVE_SUBSCRIBERS
            }),
            draft.request_to_join,
            cx.listener(|this, &on, _, cx| {
                this.edit_group_type_draft(cx, |draft| draft.request_to_join = on);
            }),
        ));
    }
    let about_key = if facts.discussion {
        request_to_join_about(draft.join_to_write, facts.group)
    } else if facts.group {
        keys::MANAGE_PEER_SEND_APPROVE_MEMBERS_ABOUT
    } else {
        keys::MANAGE_PEER_SEND_APPROVE_MEMBERS_ABOUT_CHANNEL
    };
    Some(section.child(about(tr(about_key), cx)).into_any_element())
}

/// "Content protection" / "Restrict saving content".
fn group_type_no_forwards(
    type_box: &GroupTypeBox,
    facts: &TypeBoxFacts,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(heading(tr(keys::MANAGE_PEER_NO_FORWARDS_TITLE), cx))
        .child(switch_row(
            "group-type-no-forwards",
            tr(keys::MANAGE_PEER_NO_FORWARDS),
            type_box.draft.no_forwards,
            cx.listener(|this, &on, _, cx| {
                this.edit_group_type_draft(cx, |draft| draft.no_forwards = on);
            }),
        ))
        .child(about(
            tr(if facts.group {
                keys::MANAGE_PEER_NO_FORWARDS_ABOUT
            } else {
                keys::MANAGE_PEER_NO_FORWARDS_ABOUT_CHANNEL
            }),
            cx,
        ))
        .into_any_element()
}

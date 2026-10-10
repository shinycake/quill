//! invite links + join requests.

use super::app::QuillApp;
use super::groups::apply_ready_channels_admin;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::GroupsPurpose;
use quill::state::{MemberListFilter, RequestPurpose, Session, unix_ms_now};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyInviteLinks` fixture (Phase D3a): like `apply_ready_channels_admin`
/// (chat 13, viewer 777 has `can_invite_users`), plus a `chatInviteLinks`
/// response and a `chatJoinRequests` response through the real reducer
/// paths, so the info-panel sections render loaded data. Includes a
/// primary link, a named expiring limited link, and a join-request link
/// with two pending requests (with `updateUser` names).
pub(super) fn apply_ready_invite_links(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_channels_admin(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let links_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));
    let requests_extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));
    let counts_extra = session.request(RequestPurpose::GetChatInviteLinkCounts, Some(ChatId(13)));
    let revoked_extra =
        session.request(RequestPurpose::GetRevokedChatInviteLinks, Some(ChatId(13)));
    let members_extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatInviteLinkMembers { append: false }),
        Some(ChatId(13)),
    );
    session.invite_link_members.insert(
        13,
        quill::state::InviteLinkMembersState {
            invite_link: "https://t.me/+moderatorslink".into(),
            total_count: 0,
            members: Vec::new(),
            loading: true,
            error: None,
            request: Some(members_extra),
        },
    );
    let link = |invite_link: &str,
                name: &str,
                expiration_date: i64,
                member_limit: i32,
                member_count: i32,
                pending: i32,
                join_request: bool,
                primary: bool| {
        format!(
            r#"{{"@type":"chatInviteLink","invite_link":"{invite_link}","name":"{name}","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":{expiration_date},"subscription_pricing":null,"member_limit":{member_limit},"member_count":{member_count},"expired_member_count":0,"pending_join_request_count":{pending},"creates_join_request":{join_request},"is_primary":{primary},"is_revoked":false}}"#
        )
    };
    let jsons = [
        format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":4,"invite_links":[{},{},{},{}]}}"#,
            links_extra.0,
            link("https://t.me/+primarylink", "", 0, 0, 1204, 0, false, true),
            link("https://t.me/+moderatorslink", "Moderators", 1_792_756_800, 25, 8, 0, false, false),
            link("https://t.me/+joinapprovallink", "Join approval", 0, 0, 0, 2, true, false),
            link("https://t.me/+viplink", "VIP", 0, 0, 3, 0, false, false)
                .replace(r#""subscription_pricing":null"#, r#""subscription_pricing":{"@type":"starSubscriptionPricing","period":2592000,"star_count":250}"#),
        ),
        format!(
            r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
            revoked_extra.0,
            link("https://t.me/+oldcampaign", "Old campaign", 0, 0, 41, 0, false, false)
                .replace(r#""is_revoked":false"#, r#""is_revoked":true"#),
            link("https://t.me/+spring", "Spring promo", 0, 0, 12, 0, false, false)
                .replace(r#""is_revoked":false"#, r#""is_revoked":true"#),
        ),
        format!(
            r#"{{"@type":"chatInviteLinkCounts","@extra":"{}","invite_link_counts":[{{"@type":"chatInviteLinkCount","user_id":777,"invite_link_count":4,"revoked_invite_link_count":2}},{{"@type":"chatInviteLinkCount","user_id":7001,"invite_link_count":2,"revoked_invite_link_count":0}},{{"@type":"chatInviteLinkCount","user_id":7002,"invite_link_count":1,"revoked_invite_link_count":3}}]}}"#,
            counts_extra.0,
        ),
        format!(
            r#"{{"@type":"chatInviteLinkMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatInviteLinkMember","user_id":7001,"joined_chat_date":1788600000,"via_chat_folder_invite_link":false,"approver_user_id":0}},{{"@type":"chatInviteLinkMember","user_id":7002,"joined_chat_date":1788650000,"via_chat_folder_invite_link":false,"approver_user_id":0}}]}}"#,
            members_extra.0,
        ),
        format!(
            r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi, I would like to join the channel."}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Long-time reader."}}]}}"#,
            requests_extra.0,
        ),
        r#"{"@type":"updateUser","user":{"@type":"user","id":7001,"first_name":"Dana","last_name":"Levi","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
        r#"{"@type":"updateUser","user":{"@type":"user","id":7002,"first_name":"Omar","last_name":"Haddad","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyAdminManagement` fixture (Phase D3b): like
/// `apply_ready_channels_admin` (chat 13, viewer 777 is an administrator
/// with `can_promote_members: true`), plus a `chatAdministrators`
/// response (owner with custom title + two editable admins) and a
/// `chatMembers` response for the promote picker, all through the real
/// reducer paths. `updateUser` rows give the admins/members display
/// names.
pub(super) fn apply_ready_admin_management(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_channels_admin(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let admins_extra = session.request(RequestPurpose::GetChatAdministrators, Some(ChatId(13)));
    let members_extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
            filter: MemberListFilter::Recent,
        }),
        Some(ChatId(13)),
    );
    let user = |id: i64, first: &str, last: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    let jsons = [
        user(1, "Idan", "Founder"),
        user(2, "Dana", "Levi"),
        user(777, "Demo", "Admin"),
        user(5, "Omar", "Haddad"),
        user(6, "Maya", "Sharon"),
        format!(
            r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":1,"custom_title":"Founder","is_owner":true,"can_be_edited":false}},{{"@type":"chatAdministrator","user_id":777,"custom_title":"","is_owner":false,"can_be_edited":true}},{{"@type":"chatAdministrator","user_id":2,"custom_title":"Mod","is_owner":false,"can_be_edited":true}}]}}"#,
            admins_extra.0
        ),
        format!(
            r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":5}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember"}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":6}},"tag":"","inviter_user_id":0,"joined_chat_date":0,"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
            members_extra.0
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadyAdminLog` fixture (Phase D3c): like `apply_ready_channels_admin`
/// (chat 13, viewer 777 is an administrator), plus `updateUser` rows for
/// the actors/targets and a `chatEvents` response through the real
/// reducer paths. Event timestamps are relative to capture time so the
/// rows show "just now" / "Nm ago". Covers the handled action types
/// plus one unhandled (`chatEventMemberLeft`) rendering as a generic row.
pub(super) fn apply_ready_admin_log(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_channels_admin(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let log_extra = session.request(
        RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }),
        Some(ChatId(13)),
    );
    let user = |id: i64, first: &str, last: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    let now = unix_ms_now() / 1000;
    let date = |minutes_ago: u64| now - minutes_ago * 60;
    let sender = |id: i64| format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#);
    let link = |url: &str, name: &str| {
        format!(
            r#"{{"@type":"chatInviteLink","invite_link":"{url}","name":"{name}","creator_user_id":777,"date":1700000000,"edit_date":0,"expiration_date":0,"member_limit":0,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#
        )
    };
    let event = |id: i64, minutes_ago: u64, actor: i64, action: &str| {
        format!(
            r#"{{"@type":"chatEvent","id":{id},"date":{},"member_id":{},"action":{}}}"#,
            date(minutes_ago),
            sender(actor),
            action
        )
    };
    let member_status = r#"{"@type":"chatMemberStatusMember","member_until_date":0}"#;
    let admin_status = r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":true}"#;
    let restricted_status = r#"{"@type":"chatMemberStatusRestricted"}"#;
    let events = [
        event(
            901,
            2,
            1,
            &format!(
                r#"{{"@type":"chatEventMemberPromoted","user_id":2,"old_status":{member_status},"new_status":{admin_status}}}"#
            ),
        ),
        event(
            902,
            9,
            777,
            r#"{"@type":"chatEventTitleChanged","old_title":"Demo channel","new_title":"Demo channel — news"}"#,
        ),
        event(
            903,
            21,
            1,
            &format!(
                r#"{{"@type":"chatEventMemberRestricted","member_id":{},"old_status":{member_status},"new_status":{restricted_status}}}"#,
                sender(6)
            ),
        ),
        event(
            904,
            35,
            5,
            &format!(
                r#"{{"@type":"chatEventMemberJoinedByInviteLink","invite_link":{},"via_chat_folder_invite_link":false}}"#,
                link("https://t.me/+mods", "Mods")
            ),
        ),
        event(
            905,
            58,
            777,
            r#"{"@type":"chatEventMessagePinned","message":{"id":201,"chat_id":13,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Broadcast one — channel post from the channel itself.","entities":[]}}}}"#,
        ),
        event(
            906,
            95,
            777,
            &format!(
                r#"{{"@type":"chatEventInviteLinkRevoked","invite_link":{}}}"#,
                link("https://t.me/+old", "Old campaign")
            ),
        ),
        event(
            907,
            130,
            1,
            r#"{"@type":"chatEventDescriptionChanged","old_description":"","new_description":"Daily news, no noise."}"#,
        ),
        event(
            908,
            180,
            777,
            &format!(
                r#"{{"@type":"chatEventMemberInvited","user_id":6,"status":{member_status}}}"#
            ),
        ),
        event(
            909,
            240,
            6,
            r#"{"@type":"chatEventMemberLeft"}"#,
        ),
        event(
            910,
            400,
            777,
            r#"{"@type":"chatEventPhotoChanged","old_photo":{"@type":"chatPhoto"},"new_photo":{"@type":"chatPhoto"}}"#,
        ),
    ]
    .join(",");
    let jsons = [
        user(777, "Demo", "Viewer"),
        user(1, "Idan", "Founder"),
        user(2, "Dana", "Levi"),
        user(5, "Omar", "Haddad"),
        user(6, "Maya", "Sharon"),
        format!(
            r#"{{"@type":"chatEvents","@extra":"{}","events":[{events}]}}"#,
            log_extra.0
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089) — revokes the current primary link and creates a fresh
    /// one; the new link arrives as `updateChatInviteLink`.
    pub(super) fn replace_primary_invite_link(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            Some(live) => match live.driver.replace_primary_chat_invite_link(chat_id) {
                Ok(_) => "replacing primary invite link".into(),
                Err(_) => "could not replace invite link".into(),
            },
            None => "invite links need a live connection (demo)".into(),
        };
        self.status_note = note;
        cx.notify();
    }

    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188).
    pub(super) fn toggle_join_by_request(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let current = self.chat_join_by_request(chat_id);
        let note = match self.live.as_mut() {
            Some(live) => {
                match live
                    .driver
                    .toggle_supergroup_join_by_request(chat_id, !current)
                {
                    Ok(_) => {
                        if current {
                            "join requests disabled".into()
                        } else {
                            "join requests enabled".into()
                        }
                    }
                    Err(_) => "could not toggle join requests".into(),
                }
            }
            None => "join requests need a live connection (demo)".into(),
        };
        self.status_note = note;
        cx.notify();
    }

    /// Phase D3a: open the invite-link create dialog for a chat.
    pub(super) fn open_invite_link_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let allow_subscription = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.chat_supports_subscription_links(chat_id))
            || self
                .session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .is_some_and(|chat| chat.kind.is_channel());
        self.invite_link_dialog = Some(InviteLinkDialog::new(
            window,
            cx,
            chat_id,
            allow_subscription,
        ));
        cx.notify();
    }

    /// B8: rename a subscription link (the only editable field).
    pub(super) fn open_subscription_link_rename(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut dialog = InviteLinkDialog::new(window, cx, chat_id, false);
        dialog.edit_link = Some(invite_link.to_owned());
        dialog
            .name_input
            .update(cx, |input, cx| input.set_value(name.to_owned(), window, cx));
        self.invite_link_dialog = Some(dialog);
        cx.notify();
    }

    /// Phase D3a: close the invite-link create dialog.
    pub(super) fn close_invite_link_dialog(&mut self, cx: &mut Context<Self>) {
        self.invite_link_dialog = None;
        cx.notify();
    }

    /// Phase D3a: validate the dialog and send `createChatInviteLink`.
    pub(super) fn submit_invite_link_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.invite_link_dialog.as_ref() {
            let chat_id = dialog.chat_id;
            let name = dialog.name_input.read(cx).value().to_string();
            let edit_link = dialog.edit_link.clone();
            let stars_text = dialog.stars_input.read(cx).value().to_string();
            let stars: i64 = stars_text.trim().parse().unwrap_or(0);
            if edit_link.is_some() || (dialog.allow_subscription && stars > 0) {
                let result = match (self.live.as_mut(), edit_link) {
                    (Some(live), Some(link)) => Some(
                        live.driver
                            .edit_chat_subscription_invite_link(chat_id, &link, &name),
                    ),
                    (Some(live), None) => Some(
                        live.driver
                            .create_chat_subscription_invite_link(chat_id, &name, stars),
                    ),
                    (None, _) => None,
                };
                self.invite_link_dialog = None;
                self.status_note = match result {
                    Some(Ok(_)) => "saving invite link…".into(),
                    Some(Err(_)) => "could not save invite link".into(),
                    None => "invite links need a live connection (demo)".into(),
                };
                cx.notify();
                return;
            }
            if !stars_text.trim().is_empty() && stars_text.trim().parse::<i64>().is_err() {
                self.status_note = "Stars price must be a whole number".into();
                cx.notify();
                return;
            }
        }
        let (chat_id, name, expiration_date, member_limit, creates_join_request) =
            match self.invite_link_dialog.as_ref() {
                Some(dialog) => {
                    let name = dialog.name_input.read(cx).value().to_string();
                    let days = dialog.expiration_days_input.read(cx).value().to_string();
                    let limit = dialog.member_limit_input.read(cx).value().to_string();
                    let days: i64 = match days.trim().parse() {
                        Ok(days) if days >= 0 => days,
                        _ => {
                            self.status_note =
                                "expiration must be a non-negative number of days".into();
                            cx.notify();
                            return;
                        }
                    };
                    let member_limit: i32 = match limit.trim().parse() {
                        Ok(limit) if limit >= 0 => limit,
                        _ => {
                            self.status_note = "member limit must be a non-negative number".into();
                            cx.notify();
                            return;
                        }
                    };
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_secs() as i64)
                        .unwrap_or(0);
                    let expiration_date = if days == 0 {
                        0
                    } else {
                        now.saturating_add(days.saturating_mul(86_400))
                            .min(i64::from(i32::MAX)) as i32
                    };
                    (
                        dialog.chat_id,
                        name,
                        expiration_date,
                        member_limit,
                        dialog.creates_join_request,
                    )
                }
                None => return,
            };
        if let Some(live) = self.live.as_mut() {
            match live.driver.create_chat_invite_link(
                chat_id,
                &name,
                expiration_date,
                member_limit,
                creates_join_request,
            ) {
                Ok(_) => {
                    self.invite_link_dialog = None;
                    self.status_note = "creating invite link…".into();
                }
                Err(_) => {
                    self.status_note = "could not create invite link".into();
                }
            }
        } else {
            // Screenshot demos have no live driver; close the dialog honestly.
            self.invite_link_dialog = None;
            self.status_note = "invite links need a live connection (demo)".into();
        }
        let _ = window;
        cx.notify();
    }

    /// Phase D3a: the invite-link creation dialog, rendered above the
    /// composer like the poll dialog.
    pub(super) fn invite_link_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.invite_link_dialog.as_ref()?;
        let creates_join_request = dialog.creates_join_request;
        let editing = dialog.edit_link.is_some();
        let title = if editing {
            "Rename subscription link"
        } else {
            "New invite link"
        };
        let panel = div()
            .id("invite-link-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(div().text_sm().font_semibold().child(title))
            .child(
                Textarea::new(&dialog.name_input)
                    .aria_label("Invite link name")
                    .h(px(40.)),
            )
            .when(!editing, |panel| {
                panel
                    .child(
                        Textarea::new(&dialog.expiration_days_input)
                            .aria_label("Invite link duration in days")
                            .h(px(40.)),
                    )
                    .child(
                        Textarea::new(&dialog.member_limit_input)
                            .aria_label("Invite link member limit")
                            .h(px(40.)),
                    )
                    .child(
                        // Phase 6: kit Checkbox (was: ghost button with a
                        // check label). Controlled: writes the requested
                        // value.
                        Checkbox::new("invite-link-dialog-toggle-join-request")
                            .label("Approval required to join")
                            .checked(creates_join_request)
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(dialog) = this.invite_link_dialog.as_mut() {
                                    dialog.creates_join_request = on;
                                }
                                cx.notify();
                            })),
                    )
            })
            .when(dialog.allow_subscription && !editing, |panel| {
                // tdesktop "Require Monthly Fee": a Stars price makes a
                // 30-day subscription link; expiry and limit do not apply.
                panel
                    .child(
                        Textarea::new(&dialog.stars_input)
                            .aria_label("Stars per month")
                            .h(px(40.)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("A Stars price charges people monthly to join through this link. Expiry, limit and approval do not apply."),
                    )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("invite-link-dialog-create")
                            .label(if editing { "Save" } else { "Create link" })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_invite_link_dialog(window, cx);
                            })),
                    )
                    .child(
                        Button::new("invite-link-dialog-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_invite_link_dialog(cx);
                            })),
                    ),
            );
        Some(panel.into_any_element())
    }
}

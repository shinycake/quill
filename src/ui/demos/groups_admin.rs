//! Screenshot demos: groups admin.

use crate::ui::app::QuillApp;
use crate::ui::group_invites::{
    apply_ready_admin_log, apply_ready_admin_management, apply_ready_invite_links,
};
use crate::ui::groups::{apply_ready_group_manage, apply_ready_groups2};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::statistics::apply_ready_channel_stats;
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::InfoPanelTarget;
use std::sync::atomic::Ordering;

register_demos![
    // Phase D3c: synthetic admin-log surface (no live TDLib): the demo
    // channel (id 13) with the viewer as an administrator, plus a
    // loaded `chatEvents` fixture covering the handled action types, so
    // the info panel's "Recent actions" section renders directly.
    DemoSpec::chats("ready-admin-log", "screenshot demo — recent actions")
        .setup(QuillApp::demo_ready_admin_log),
    // Phase D3b: synthetic admin-management surface (no live TDLib):
    // the demo channel (id 13) with the viewer as an admin with
    // `can_promote_members`, a loaded administrator list (owner +
    // two editable admins), a loaded member page for the promote
    // picker, and the promote dialog open with a member selected, so
    // the info-panel section and the rights checkboxes render directly.
    DemoSpec::chats(
        "ready-admin-management",
        "screenshot demo — admin management"
    )
    .setup(QuillApp::demo_ready_admin_management),
    // Avatar click in a group (injected, no live Telegram): a member's
    // profile open as the modal layer over the group history, as after
    // clicking the avatar next to their message.
    DemoSpec::chats(
        "ready-avatar-profile",
        "screenshot demo — group member profile from an avatar click"
    )
    .setup(QuillApp::demo_ready_avatar_profile),
    // Channel statistics demo (injected, no live Telegram): the demo
    // channel (id 13) with `supergroupFullInfo.can_get_statistics: true`
    // and a loaded `chatStatisticsChannel` fixture (Phase D2), so the
    // statistics panel renders directly in the info panel.
    DemoSpec::chats(
        "ready-channel-stats",
        "screenshot demo — channel statistics"
    )
    .setup(QuillApp::demo_ready_channel_stats),
    // Slice G10: communities create dialog (injected, no live
    // Telegram): the create dialog open over the seeded chat list, so
    // the name field, chat picker, and hide-checkbox render directly.
    DemoSpec::chats(
        "ready-community-create",
        "screenshot demo — communities G10"
    )
    .setup(|app, window, cx| app.demo_community(CommunityDemo::Create, window, cx)),
    // Slice G10: communities hub dialog (injected, no live Telegram):
    // two injected communities with the hub open.
    DemoSpec::chats("ready-community-hub", "screenshot demo — communities G10")
        .setup(|app, window, cx| app.demo_community(CommunityDemo::Hub, window, cx)),
    // Slice G10: community info panel (injected, no live Telegram):
    // the "Rustaceans" community with its injected full-info pack, so
    // the name edit, counts, and chat rows render directly.
    DemoSpec::chats("ready-community-info", "screenshot demo — communities G10")
        .setup(|app, window, cx| app.demo_community(CommunityDemo::Info, window, cx)),
    // Slice G8: group info-edit demo (injected, no live Telegram):
    // the `ReadyGroupManage` demo supergroup (id 61, viewer an admin
    // with `can_change_info`) with the info panel open, so the new
    // Edit title / Edit description / Change photo rows render
    // directly.
    DemoSpec::chats("ready-group-info-edit", "screenshot demo — group info edit")
        .setup(QuillApp::demo_ready_group_info_edit),
    // Slice G1: synthetic group-management surface (no live TDLib):
    // demo supergroup (id 61) with the viewer as an administrator
    // (`can_restrict_members`, `can_invite_users`, `can_manage_tags`),
    // a loaded `chatMembers` page (member, admin with custom title,
    // restricted member), and the member-management dialog open on the
    // All tab, so member rows, custom titles, and per-tab actions
    // render directly.
    DemoSpec::chats("ready-group-manage", "screenshot demo — group management")
        .setup(QuillApp::demo_ready_group_manage),
    // Slice G2: channel-management surface (no live TDLib): like
    // `ReadyAdminLog` (demo channel id 13, viewer 777 is an admin), plus
    // signature flags (`sign_messages` on, `show_message_sender` off),
    // a seeded boost status, the `can_send_welcome_messages` right, and
    // a loaded one-message welcome pack, so the info panel's signatures,
    // boost, welcome-message, and recent-actions sections render
    // directly.
    DemoSpec::chats("ready-groups2", "screenshot demo — groups/channels G2")
        .setup(QuillApp::demo_ready_groups2),
    // Phase D3a: synthetic invite-links + join-requests surface (no live
    // TDLib): the demo channel (id 13) with the viewer as an admin with
    // `can_invite_users`, a loaded invite-link list and loaded join
    // requests, so the info-panel sections render directly.
    DemoSpec::chats("ready-invite-links", "screenshot demo — invite links")
        .setup(QuillApp::demo_ready_invite_links),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommunityDemo {
    Create,
    Hub,
    Info,
}

impl QuillApp {
    fn demo_ready_admin_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase D3c: admin-log fixture, then open the channel info panel
        // (viewer 777 is an administrator, seeded by
        // apply_ready_channels_admin).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_admin_log(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
        self.status_note = "screenshot demo — recent actions".into();
    }

    fn demo_ready_admin_management(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase D3b: admin-management fixture, then open the channel info
        // panel plus the promote picker (viewer 777 has
        // can_promote_members, seeded by apply_ready_channels_admin).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_admin_management(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
        self.admin_dialog = Some(AdminDialog::promote(window, cx, ChatId(13)));
        if let Some(dialog) = self.admin_dialog.as_mut()
            && let AdminDialogKind::Promote { selected_user, .. } = &mut dialog.kind
        {
            *selected_user = Some(5);
        }
        self.status_note = "screenshot demo — admin management".into();
    }

    fn demo_ready_avatar_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Avatar click: group history with a member's profile layer open.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::profile_modal::apply_ready_avatar_profile(
                session,
                &self.demo_sink,
                &self.demo_seq,
            );
        }
        self.open_avatar_profile(
            quill::telegram::envelope::MessageSender::User { user_id: 602 },
            window,
            cx,
        );
        // Captures show the settled layer, not the fade.
        self.profile_modal = Some(crate::ui::profile_modal::ProfileModal::shown(
            InfoPanelTarget::User(602),
        ));
        self.status_note = "screenshot demo — profile layer from an avatar click".into();
    }

    fn demo_ready_channel_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase D2: channel statistics fixture, then open the stats panel
        // directly in the info panel (demo path just sets the target).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channel_stats(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Statistics(13), window, cx);
        self.status_note = "screenshot demo — channel statistics".into();
    }

    fn demo_ready_group_info_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Slice G8: same group-management fixture (viewer 777 is an
        // admin with `can_change_info`), but open the info panel
        // instead of the member dialog so the Edit title / Edit
        // description / Change photo rows render directly.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(61), window, cx);
        self.status_note = "screenshot demo — group info edit".into();
        cx.notify();
    }

    fn demo_ready_group_manage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Slice G1: group-management fixture, then open the member
        // dialog on the demo supergroup (viewer 777 is an admin with
        // restrict/invite/tag rights, seeded by
        // apply_ready_group_manage).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_sink, &self.demo_seq);
        }
        self.member_dialog = Some(MemberDialog::new(window, cx, ChatId(61), false));
        self.status_note = "screenshot demo — group management".into();
        cx.notify();
    }

    fn demo_ready_groups2(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Slice G2: channel-management fixture, then open the info panel
        // on the demo channel so the signatures, boost, welcome-message,
        // and recent-actions sections render directly.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_groups2(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
        self.status_note = "screenshot demo — groups/channels G2".into();
        cx.notify();
    }

    fn demo_ready_invite_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase D3a: invite-links fixture, then open the channel info panel
        // (admin with can_invite_users, seeded by apply_ready_channels_admin).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_invite_links(session, &self.demo_sink, &self.demo_seq);
        }
        self.invite_link_details = Some((
            quill::ids::ChatId(13),
            "https://t.me/+moderatorslink".to_owned(),
        ));
        self.revoked_links_open = true;
        self.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx);
        self.status_note = "screenshot demo — invite links".into();
    }

    fn demo_community(&mut self, demo: CommunityDemo, window: &mut Window, cx: &mut Context<Self>) {
        // Slice G10: community fixtures, then open the new surface.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            community::apply_ready_communities(session, &self.demo_sink, &self.demo_seq);
        }
        match demo {
            CommunityDemo::Create => {
                self.open_create_community_dialog(window, cx);
            }
            CommunityDemo::Hub => {
                self.open_community_hub(cx);
            }
            _ => {
                self.open_info_panel_target(InfoPanelTarget::Community(9001), window, cx);
            }
        }
        self.status_note = "screenshot demo — communities G10".into();
        cx.notify();
    }
}

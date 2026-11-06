//! Admin management boxes over the window: pick a member to promote
//! (tdesktop `AddParticipantsBoxController` then `EditAdminBox`), edit an
//! administrator's rights (`EditAdminBox`) and confirm a demotion. They
//! used to be one panel pushed in above the composer that Escape could not
//! close.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::confirm_boxes::{Ask, confirm_box};
use super::dialog_focus::focus_after_mount;
use super::dialogs::{AdminDialog, AdminDialogKind};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};

/// tdesktop `boxWideWidth`: two columns of rights checkboxes.
const BOX_WIDTH: f32 = 440.;

fn admin_kind(app: &QuillApp) -> Option<&AdminDialogKind> {
    app.admin.admin_dialog.as_ref().map(|dialog| &dialog.kind)
}

impl QuillApp {
    fn admin_user_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| tr_with(keys::Q_PEOPLE_USER, &[("user_id", &user_id.to_string())]))
    }

    /// Cancel and the primary button under the box. Promote stays disabled
    /// until a member is picked.
    fn admin_box_footer(&self, label: SharedString, ready: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("admin-box-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_admin_dialog(cx))),
            )
            .child(
                Button::new("admin-box-submit")
                    .label(label)
                    .primary()
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.submit_admin_dialog(cx))),
            )
    }

    /// The box frame shared by promote and edit rights: title, scrolling
    /// body, footer; Enter submits, Escape and ✕ cancel.
    fn admin_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        kind: DialogKind,
        (title, label, ready): (SharedString, SharedString, bool),
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, kind, |this, _, cx| {
            this.close_admin_dialog(cx);
        });
        let footer = app.update(cx, |this, cx| this.admin_box_footer(label, ready, cx));
        let ok_app = app.clone();
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(title))
            .on_ok(move |_, _, cx| {
                if ready {
                    ok_app.update(cx, |this, cx| this.submit_admin_dialog(cx));
                }
                false
            })
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, window, cx| {
                    let body = body_app.update(cx, |this, cx| {
                        let focus = match this.admin.admin_dialog.as_mut().map(|d| &mut d.kind) {
                            Some(AdminDialogKind::Promote {
                                search_input,
                                focus_on_mount,
                                ..
                            }) => std::mem::take(focus_on_mount).then(|| search_input.clone()),
                            _ => None,
                        };
                        focus_after_mount(focus, window, cx);
                        this.admin_box_body(cx)
                    });
                    content.children(body)
                },
            ))
            .footer(footer)
            .on_close(on_close)
    }

    fn admin_box_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let AdminDialog { chat_id, kind } = self.admin.admin_dialog.as_ref()?;
        Some(match kind {
            AdminDialogKind::Promote {
                search_input,
                rights,
                selected_user,
                ..
            } => self.promote_dialog_panel(*chat_id, search_input, rights, *selected_user, cx),
            AdminDialogKind::EditRights { user_id, rights } => {
                self.rights_editor_panel(*chat_id, *user_id, *rights, cx)
            }
            AdminDialogKind::DemoteConfirm { .. } => return None,
        })
    }

    fn build_admin_promote_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let ready = matches!(
            admin_kind(app.read(cx)),
            Some(AdminDialogKind::Promote {
                selected_user: Some(_),
                ..
            })
        );
        let text = (
            tr(keys::Q_PEOPLE_PROMOTE_MEMBER).into(),
            tr(keys::COMMUNITY_ADMIN_PROMOTE).into(),
            ready,
        );
        Self::admin_box(app, shell, dialog, DialogKind::AdminPromote, text, cx)
    }

    fn build_admin_rights_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let this = app.read(cx);
        let name = match admin_kind(this) {
            Some(AdminDialogKind::EditRights { user_id, .. }) => this.admin_user_name(*user_id),
            _ => String::new(),
        };
        let text = (
            tr_with(keys::Q_PEOPLE_EDIT_RIGHTS, &[("name", &name)]).into(),
            tr(keys::SETTINGS_SAVE).into(),
            true,
        );
        Self::admin_box(app, shell, dialog, DialogKind::AdminRights, text, cx)
    }

    fn build_admin_demote_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let this = app.read(cx);
        let name = match admin_kind(this) {
            Some(AdminDialogKind::DemoteConfirm { user_id }) => this.admin_user_name(*user_id),
            _ => String::new(),
        };
        let ask = Ask {
            title: tr_with(keys::Q_PEOPLE_DEMOTE, &[("name", &name)]).into(),
            text: tr(keys::Q_PEOPLE_THEY_WILL_BECOME_A_REGULAR).into(),
            confirm: tr(keys::Q_PEOPLE_DEMOTE_2).into(),
        };
        confirm_box(
            app,
            shell,
            dialog,
            DialogKind::AdminDemote,
            ask,
            (Self::close_admin_dialog, Self::submit_admin_dialog),
            cx,
        )
    }
}

crate::ui::shell::register_dialogs! {
    AdminPromote => DialogSpec::new(
        2948,
        |app| matches!(admin_kind(app), Some(AdminDialogKind::Promote { .. })),
        QuillApp::build_admin_promote_dialog,
    ),
    AdminRights => DialogSpec::new(
        2949,
        |app| matches!(admin_kind(app), Some(AdminDialogKind::EditRights { .. })),
        QuillApp::build_admin_rights_dialog,
    ),
    AdminDemote => DialogSpec::new(
        2951,
        |app| matches!(admin_kind(app), Some(AdminDialogKind::DemoteConfirm { .. })),
        QuillApp::build_admin_demote_dialog,
    ),
}

super::screenshot_demo::register_demos![
    // `QUILL_DEMO_ADMIN=promote` (default) picks a member to promote,
    // `rights` edits an admin's rights, `demote` confirms a demotion.
    super::screenshot_demo::DemoSpec::chat_list("ready-admin-box").setup(QuillApp::demo_admin_box),
];

impl QuillApp {
    fn demo_admin_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use quill::ids::ChatId;
        use quill::state::{MemberListFilter, SupergroupMembersFetch};
        use quill::telegram::envelope::{
            ChannelMemberStatus, ChatAdminRights, MessageSender, ParsedChatMember,
        };
        let chat = ChatId(12);
        if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat);
            super::composer_date::demo_contacts(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let members = [41, 42, 43]
                .into_iter()
                .map(|user_id| ParsedChatMember {
                    member_id: MessageSender::User { user_id },
                    status: ChannelMemberStatus::Member,
                    admin_can_post_messages: None,
                    admin_can_invite_users: None,
                    admin_rights: None,
                    tag: String::new(),
                    can_be_edited: true,
                    restriction: None,
                    inviter_user_id: 0,
                    joined_chat_date: 0,
                })
                .collect();
            session.groups.supergroup_members.insert(
                (chat.0, MemberListFilter::Recent),
                SupergroupMembersFetch::Loaded {
                    members,
                    total_count: 3,
                },
            );
        }
        self.admin.admin_dialog = Some(match std::env::var("QUILL_DEMO_ADMIN").as_deref() {
            Ok("rights") => AdminDialog::edit_rights(chat, 42, Some(ChatAdminRights::all())),
            Ok("demote") => AdminDialog::demote_confirm(chat, 43),
            _ => {
                let mut dialog = AdminDialog::promote(window, cx, chat);
                if let AdminDialogKind::Promote { selected_user, .. } = &mut dialog.kind {
                    *selected_user = Some(42);
                }
                dialog
            }
        });
        cx.notify();
    }
}

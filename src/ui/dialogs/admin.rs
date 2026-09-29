use super::super::*;

/// Phase D3b: admin-management dialog above the composer. Three flows
/// share one slot:
/// - `Promote`: member picker (search + member list from
///   `getSupergroupMembers`) followed by the rights checkboxes; the
///   checkboxes start with every right enabled (mirrors the official
///   clients' "all rights" default).
/// - `EditRights`: rights checkboxes for an existing administrator,
///   pre-filled from `Session::admin_rights` once the `getChatMember`
///   lookup lands (`rights` stays `None` until then).
/// - `DemoteConfirm`: demote confirmation for one administrator.
pub struct AdminDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) kind: AdminDialogKind,
}

pub enum AdminDialogKind {
    Promote {
        search_input: Entity<TextareaState>,
        rights: ChatAdminRights,
        selected_user: Option<i64>,
    },
    EditRights {
        user_id: i64,
        rights: Option<ChatAdminRights>,
    },
    DemoteConfirm {
        user_id: i64,
    },
}

impl AdminDialog {
    pub(crate) fn promote(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
    ) -> Self {
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search members")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            kind: AdminDialogKind::Promote {
                search_input,
                rights: ChatAdminRights::all(),
                selected_user: None,
            },
        }
    }

    pub(crate) fn edit_rights(
        chat_id: ChatId,
        user_id: i64,
        rights: Option<ChatAdminRights>,
    ) -> Self {
        Self {
            chat_id,
            kind: AdminDialogKind::EditRights { user_id, rights },
        }
    }

    pub(crate) fn demote_confirm(chat_id: ChatId, user_id: i64) -> Self {
        Self {
            chat_id,
            kind: AdminDialogKind::DemoteConfirm { user_id },
        }
    }
}

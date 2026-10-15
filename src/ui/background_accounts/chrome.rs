//! Where the other accounts show outside the Accounts list: the account
//! name in the window title, and the unread dot on the main menu button
//! with the counter on its "Accounts" item.

use super::*;
use crate::ui::{accent_strong, bg_badge_muted};
use gpui_kit::component::badge::Badge;
use quill::lang::{keys, tr};
use quill::notify_accounts::account_name_in_title;
use quill::telegram::envelope::AuthorizationState;

impl QuillApp {
    /// The signed-in user's name for the window title, while two or more
    /// accounts are signed in (tdesktop `authedName`; a background account
    /// that is restoring its session counts, one that needs a sign-in
    /// does not).
    pub(in crate::ui) fn title_account_name(&self) -> Option<String> {
        let session = &self.live.as_ref()?.driver.session;
        if !matches!(session.auth, AuthorizationState::Ready) {
            return None;
        }
        let others = self
            .background
            .clients
            .iter()
            .filter(|client| client.live.is_some() && !client.status.stops_client())
            .count();
        if !account_name_in_title(1 + others) {
            return None;
        }
        let me = session.my_user_id?;
        session.user(me).map(|user| user.display_name())
    }

    /// The main menu button with tdesktop's unread dot while another
    /// account has unread messages (`dialogsMenuToggleUnread`, grey when
    /// all of it is muted).
    pub(in crate::ui) fn with_other_accounts_dot(&self, button: AnyElement) -> AnyElement {
        let unread = self.background.total_unread();
        if unread.count == 0 {
            return button;
        }
        let color = if unread.muted {
            bg_badge_muted()
        } else {
            accent_strong()
        };
        Badge::new()
            .dot()
            .color(color)
            .child(button)
            .into_any_element()
    }

    /// The other accounts' unread total for the main menu's "Accounts"
    /// item (tdesktop's accounts toggle badge); `None` when there is none.
    pub(in crate::ui) fn other_accounts_unread(&self) -> Option<AccountUnread> {
        let unread = self.background.total_unread();
        (unread.count > 0).then_some(unread)
    }
}

/// The "Accounts" menu row with the other accounts' counter.
pub(in crate::ui) fn accounts_menu_row(unread: AccountUnread, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_between()
        .gap_2()
        .child(tr(keys::Q_OPS_ACCOUNTS))
        .child(super::account_row::counter_pill(
            "accounts-menu",
            unread,
            cx,
        ))
        .into_any_element()
}

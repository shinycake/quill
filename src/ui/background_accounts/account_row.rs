//! What background accounts show: the unread counter or status beside an
//! account in the Accounts list (tdesktop draws each other account's
//! `unreadWithMentionsBadge` on its row), and the "Show notifications from"
//! setting (tdesktop `BuildMultiAccountSection`).

use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::{accent_strong, text_on_fill};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use quill::lang::{keys, tr, tr_with};
use quill::notify_accounts::notify_from_all_setting_visible;

/// What a background account's row shows beside its name.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RowBadge {
    /// A status that needs saying ("Not signed in", "Signed out", why it
    /// could not start); it hides the counter.
    Status(String),
    Unread(AccountUnread),
    Nothing,
}

/// The row's badge for an account in `status` with `unread` messages.
pub(super) fn row_badge(status: &BackgroundStatus, unread: AccountUnread) -> RowBadge {
    match status.label() {
        Some(label) => RowBadge::Status(label.to_string()),
        None if unread.count > 0 => RowBadge::Unread(unread),
        None => RowBadge::Nothing,
    }
}

impl QuillApp {
    /// Unread counter (or a status such as "Not signed in") for a
    /// background account's row; `None` when there is nothing to show.
    pub(in crate::ui) fn background_account_badge(
        &self,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let client = self.background.client(key)?;
        match row_badge(&client.stage.status(), client.unread) {
            RowBadge::Status(label) => Some(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label)
                    .into_any_element(),
            ),
            RowBadge::Unread(unread) => {
                Some(counter_pill(&format!("account-unread-{key}"), unread, cx))
            }
            RowBadge::Nothing => None,
        }
    }

    /// Settings > Notifications: "Show notifications from: All accounts",
    /// only with two or more accounts.
    pub(in crate::ui) fn notify_from_all_section(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = safe_app_root().map_or(0, |root| list_accounts(&root).len());
        if !notify_from_all_setting_visible(count) {
            return None;
        }
        let label = tr(keys::Q_OPS_ALL_ACCOUNTS);
        Some(
            div()
                .id("notify-from-all-section")
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .font_semibold()
                                .text_sm()
                                .child(tr(keys::Q_OPS_SHOW_NOTIFICATIONS_FROM)),
                        )
                        .child(
                            Switch::new("notify-from-all-toggle")
                                .checked(self.background.notify_from_all)
                                .accessibility_label(label)
                                .on_click(cx.listener(|this, &on, _, cx| {
                                    this.set_notify_from_all(on, cx);
                                })),
                        ),
                )
                .child(div().text_sm().child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_TURN_THIS_OFF_TO_GET)),
                )
                .into_any_element(),
        )
    }

    /// Save "All accounts"; turning it off withdraws what the other
    /// accounts already show (tdesktop `clearFromSession`).
    fn set_notify_from_all(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(root) = safe_app_root()
            && let Err(err) = quill::settings::set_notify_from_all(&root, on)
        {
            self.set_note(StatusNote::silent(tr_with(
                keys::Q_OPS_COULDN_T_SAVE_THE_NOTIFICATION,
                &[("err", &err.to_string())],
            )));
        }
        self.background.notify_from_all = on;
        if !on {
            let mut shown: Vec<(AccountKey, ChatId)> = Vec::new();
            for client in self.background.clients_mut().iter_mut() {
                if let Some(live) = client.live_mut() {
                    let chats = live.driver.session.settings.notified.take_shown();
                    shown.extend(chats.into_iter().map(|chat| (client.key.clone(), chat)));
                }
            }
            for (key, chat_id) in shown {
                self.dismiss_notification_as(Some(&key), chat_id, cx);
            }
        }
        cx.notify();
    }
}

/// The counter pill: accent while something unmuted is unread, grey when
/// every counted chat is muted (like the folder tab counters).
pub(super) fn counter_pill(id: &str, unread: AccountUnread, cx: &App) -> AnyElement {
    let bg = if unread.muted {
        cx.theme().muted_foreground.opacity(0.55)
    } else {
        Hsla::from(accent_strong())
    };
    let label = if unread.count > 999 {
        format!("{}K", unread.count / 1000)
    } else {
        unread.count.to_string()
    };
    div()
        .id(SharedString::from(id.to_string()))
        .flex_none()
        .h(px(16.))
        .min_w(px(16.))
        .px(px(4.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(text_on_fill())
        .text_size(px(10.))
        .font_semibold()
        .aria_label(format!("{} unread", unread.count))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod badge_tests {
    use super::{AccountUnread, BackgroundStatus, RowBadge, row_badge};

    fn unread(count: u32) -> AccountUnread {
        AccountUnread {
            count,
            muted: false,
        }
    }

    /// A running account shows its unread count, or nothing when all is
    /// read; a status that needs saying replaces the count.
    #[test]
    fn a_row_shows_the_unread_count_or_the_status() {
        assert_eq!(
            row_badge(&BackgroundStatus::Ready, unread(0)),
            RowBadge::Nothing
        );
        assert_eq!(
            row_badge(
                &BackgroundStatus::Connecting {
                    since: std::time::Instant::now(),
                },
                unread(3),
            ),
            RowBadge::Unread(unread(3))
        );
        assert_eq!(
            row_badge(&BackgroundStatus::NeedsSignIn, unread(3)),
            RowBadge::Status("Not signed in".into())
        );
    }
}

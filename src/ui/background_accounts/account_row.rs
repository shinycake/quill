//! What background accounts show: the unread counter or status beside an
//! account in the Accounts list (tdesktop draws each other account's
//! `unreadWithMentionsBadge` on its row), and the "Show notifications from"
//! setting (tdesktop `BuildMultiAccountSection`).

use super::*;
use crate::ui::{accent_strong, text_on_fill};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use quill::notify_accounts::notify_from_all_setting_visible;

impl QuillApp {
    /// Unread counter (or a status such as "Not signed in") for a
    /// background account's row; `None` when there is nothing to show.
    pub(in crate::ui) fn background_account_badge(
        &self,
        key: &AccountKey,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let client = self.background.client(key)?;
        if let Some(label) = client.status.label() {
            return Some(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string())
                    .into_any_element(),
            );
        }
        (client.unread.count > 0).then(|| unread_pill(key, client.unread, cx))
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
        let label = "All accounts";
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
                                .child("Show notifications from"),
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
                        .child(
                            "Turn this off to get notifications only from the account \
                             you are using.",
                        ),
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
            self.connection.status_note = format!("couldn’t save the notification setting: {err}");
        }
        self.background.notify_from_all = on;
        if !on {
            let mut shown: Vec<(AccountKey, ChatId)> = Vec::new();
            for client in &mut self.background.clients {
                if let Some(live) = client.live.as_mut() {
                    let chats =
                        std::mem::take(&mut live.driver.session.settings.shown_notification_chats);
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
fn unread_pill(key: &AccountKey, unread: AccountUnread, cx: &App) -> AnyElement {
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
        .id(SharedString::from(format!("account-unread-{key}")))
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

//! The public links limit box's "Your public communities" list (tdesktop
//! `PublicLinksLimitBox` / `PublicsController`): each chat the account
//! made public, with Revoke to free its link.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::ids::ChatId;
use quill::state::PublicChatsFetch;

impl QuillApp {
    /// The list under the limit box's text; asks for it the first time.
    pub(super) fn public_links_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let fetch = self
            .session()
            .map(|s| s.groups.group_type.public_chats.clone())
            .unwrap_or_default();
        if fetch == PublicChatsFetch::None
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.load_created_public_chats();
        }
        let muted = cx.theme().muted_foreground;
        let mut list = div()
            .id("public-links-list")
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::LINKS_REVOKE_TITLE)),
            );
        let chats = match fetch {
            PublicChatsFetch::Loaded(chats) => chats,
            PublicChatsFetch::Failed => {
                return list
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(tr(keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY)),
                            )
                            .child(
                                Button::new("public-links-retry")
                                    .label(tr(keys::Q_CHAT_RETRY))
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(live) = this.live.as_mut() {
                                            let _ = live.driver.retry_created_public_chats();
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .into_any_element();
            }
            _ => {
                return list
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_PEOPLE_LOADING)),
                    )
                    .into_any_element();
            }
        };
        let revoking = self
            .session()
            .and_then(|s| s.groups.group_type.revoke.revoking())
            .map(|revoke| revoke.chat_id);
        for chat_id in chats {
            let Some(session) = self.session() else {
                break;
            };
            let Some(chat) = session.chats.get(&chat_id.0) else {
                continue;
            };
            let username = session
                .chat_supergroup(chat_id)
                .and_then(|sg| session.supergroup_username(sg))
                .unwrap_or_default()
                .to_string();
            let title = chat.title.clone();
            let group = !chat.is_channel();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("public-link-{}", chat_id.0)))
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(title.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .truncate()
                                    .child(format!("t.me/{username}")),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "public-link-revoke-{}",
                            chat_id.0
                        )))
                        .label(tr(keys::CHANNELS_TOO_MUCH_PUBLIC_REVOKE))
                        .ghost()
                        .small()
                        .loading(revoking == Some(chat_id))
                        .disabled(revoking.is_some())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.confirm_public_link_revoke(
                                    chat_id,
                                    title.clone(),
                                    username.clone(),
                                    group,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    ),
            );
        }
        list.into_any_element()
    }

    fn confirm_public_link_revoke(
        &mut self,
        chat_id: ChatId,
        title: String,
        username: String,
        group: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = tr_with(
            if group {
                keys::CHANNELS_TOO_MUCH_PUBLIC_REVOKE_CONFIRM_GROUP
            } else {
                keys::CHANNELS_TOO_MUCH_PUBLIC_REVOKE_CONFIRM_CHANNEL
            },
            &[("link", &format!("t.me/{username}")), ("group", &title)],
        );
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .description(text.clone())
                .ok_text(tr(keys::CHANNELS_TOO_MUCH_PUBLIC_REVOKE))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.revoke_public_link(chat_id, cx));
                    true
                })
        });
    }

    /// Revoke; once both answers came, the Type box's link is checked
    /// again (tdesktop goes back to Public and checks it).
    fn revoke_public_link(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        if live.driver.revoke_public_link(chat_id).is_err() {
            self.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY,
            )));
            cx.notify();
            return;
        }
        // The Type box checks its link again when the revoke settles
        // (`drain_public_link_freed`), not from a timer.
        cx.notify();
    }
}

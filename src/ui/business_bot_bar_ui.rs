//! The business bot manage bar above a private chat's history (tdesktop
//! `BusinessBotStatus`): the bot's photo and name, a status line, a
//! "Stop" / "Start" button and a menu with "Manage bot" and "Remove bot from
//! this chat". The state and texts live in `quill::business_bot_bar`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The bar for the open chat, or `None` when no bot manages it.
    pub(super) fn business_bot_bar_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.session()?.business_bot_bar(chat_id)?;
        self.business_bot_bar_view_body(chat_id, cx)
    }

    /// `business_bot_bar_view` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn business_bot_bar_view_body(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let bar = session.business_bot_bar(chat_id)?.clone();
        let name = self.contact_display_name(bar.bot_user_id);
        let photo = session
            .user_photo_path(bar.bot_user_id)
            .map(std::path::PathBuf::from);
        let owner = cx.entity().downgrade();
        let toggle = bar.toggle();
        let remove_label = bar.remove_label();
        let manage_url = bar.manage_url.clone();
        let menu = Button::new("business-bot-menu")
            .icon(gpui_kit::assets::IconName::EllipsisVertical)
            .ghost()
            .small()
            .tooltip(tr(keys::Q_MEDIA_BOT_ACTIONS))
            .accessibility_label(tr(keys::Q_MEDIA_BOT_ACTIONS))
            .dropdown_menu(move |mut menu, _, _| {
                if !manage_url.is_empty() {
                    let (owner, url) = (owner.clone(), manage_url.clone());
                    menu = menu.item(PopupMenuItem::new(tr(keys::MANAGE_BOT_TITLE)).on_click(
                        move |_, _, cx| {
                            let _ = owner
                                .update(cx, |this, cx| this.open_business_bot_manage(&url, cx));
                        },
                    ));
                }
                let owner = owner.clone();
                menu.item(PopupMenuItem::new(remove_label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.remove_bot_from_bar_chat(chat_id, cx));
                }))
            });
        Some(
            self.bar_shell("business-bot-bar", cx)
                .gap_3()
                .child(
                    div()
                        .flex_none()
                        .child(chat_avatar(&name, photo.as_deref(), 36.)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(text_primary())
                                .truncate()
                                .child(name),
                        )
                        .child(div().text_xs().text_color(text_muted()).child(bar.status())),
                )
                .when_some(toggle, |row, toggle| {
                    row.child(
                        Self::bar_button("business-bot-toggle".into(), toggle.label(), false)
                            .outline()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_business_bot(chat_id, toggle.target_paused(), cx);
                            })),
                    )
                })
                .child(menu)
                .into_any_element(),
        )
    }

    fn toggle_business_bot(&mut self, chat_id: ChatId, paused: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .toggle_business_bot_paused(chat_id, paused)
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_CHANGE_THE_BOT,
            )));
        }
        cx.notify();
    }

    fn remove_bot_from_bar_chat(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.remove_business_bot_from_chat(chat_id).is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_REMOVE_THE_BOT,
            )));
        }
        cx.notify();
    }

    /// "Manage bot" opens the bar's `manage_url`, a t.me link the app
    /// routes itself; anything else goes to the browser.
    fn open_business_bot_manage(&mut self, url: &str, cx: &mut Context<Self>) {
        if self.live.is_some() && url.starts_with("https://t.me/") {
            self.queue_deep_link(url.to_string());
            cx.notify();
        } else {
            self.open_link(url, cx);
        }
    }
}

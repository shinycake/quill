//! A Business account's start page in an empty chat with it (tdesktop
//! `history_view_about_view.cpp`: `AboutView::makeIntro`): its title,
//! message and sticker (clicking the sticker sends it), under "{name}
//! added the message below for all empty chats. How?", where "How?" opens
//! the start page settings, or Premium without it.

use super::View;
use crate::ui::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::business_start_page::added_line;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The start page of `user_id`, when the user set one. `None` keeps
    /// the plain greeting.
    pub(in crate::ui) fn business_start_page_intro(
        &mut self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let Some(info) = session.user_full_info(user_id) else {
            // The full info carries the start page; ask once.
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.fetch_user_full_info(user_id);
                    }
                });
            });
            return None;
        };
        let page = info.extras.business.as_ref()?.start_page.clone()?;
        let name = session
            .users
            .get(&user_id)
            .map(|u| u.display_name())
            .unwrap_or_default();
        let muted = cx.theme().muted_foreground;
        let mut card = self.business_intro_texts(&page, cx);
        // The sticker (or a random hello sticker) sends itself when
        // clicked, like the plain greeting.
        let sticker = page
            .sticker
            .clone()
            .or_else(|| self.business_random_greeting(user_id));
        if let Some(sticker) = sticker {
            if let Some(file) = sticker.display_file_id()
                && let Some(live) = self.live.as_mut()
            {
                let _ = live.driver.ensure_media_files(&[file]);
            }
            card = card.child(self.sticker_cell(&sticker, "start-page", cx));
        }
        Some(
            div()
                .id("business-start-page")
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .max_w(px(320.))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_center()
                                .gap_1()
                                .text_xs()
                                .text_color(muted)
                                .child(added_line(&name))
                                .child(
                                    Button::new("business-start-page-how")
                                        .label(tr(keys::Q_BUSINESS_START_PAGE_HOW))
                                        .ghost()
                                        .xsmall()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if this.business_premium() {
                                                this.open_business(window, cx);
                                                this.business_go(View::StartPage, window, cx);
                                            } else {
                                                this.open_premium_at(
                                                    quill::premium_hub::BusinessFeature::StartPage,
                                                    cx,
                                                );
                                            }
                                        })),
                                ),
                        )
                        .child(card),
                )
                .into_any_element(),
        )
    }
}

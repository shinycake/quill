//! The shared-media list with month headers, and the gallery's Calendar
//! button (tdesktop `Info::Media` month sections and its "Show calendar"
//! action): the box marks the days that have media and a picked day reloads
//! the tab from there.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::PressableDiv;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::state::{SharedMediaAction, SharedMediaTab, calendar_media, month_sections};
use quill::text::LinkTarget;

impl QuillApp {
    /// The `Ready` body of a tab: items under their month headers.
    pub(super) fn shared_media_ready_list(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(state) = session.media.shared_media.tab_at(tab) else {
            return div().into_any_element();
        };
        let sections = month_sections(&state.items);
        let state_chat = session.media.shared_media.chat_id;
        let mut list = div()
            .id(("shared-media-items", tab.index() as u64))
            .flex_1()
            .overflow_y_scroll()
            .py_1();
        if let Some(bar) = self.shared_media_date_bar(tab, cx) {
            list = list.child(bar);
        }
        for (index, item) in state.items.iter().enumerate() {
            if let Some(section) = sections.iter().find(|s| s.start == index) {
                list = list.child(
                    div()
                        .id(("shared-media-month", index as u64))
                        .role(gpui_kit::Role::Heading)
                        .aria_label(section.month.title())
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(section.month.title()),
                );
            }
            let message_id = item.message_id;
            let action = item.action;
            let link_url = item.meta.link_url().map(str::to_owned);
            let chat_id = state_chat;
            let is_message = action == SharedMediaAction::Message;
            let item_ix = if is_message {
                message_id.0 as u64
            } else {
                index as u64
            };
            let row = div()
                .id(("shared-media-item", item_ix))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .role(gpui_kit::Role::Button)
                .aria_label(if is_message {
                    tr_with(
                        keys::Q_OPS_OPEN_SHARED_MEDIA_MESSAGE,
                        &[("0", &message_id.0.to_string())],
                    )
                } else {
                    item.label.clone()
                })
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .child(match self.shared_media_row_lines(item) {
                    Some(lines) => self.shared_media_row_body(item, lines, cx),
                    None => div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_lg().child(item.glyph))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(item.label.clone()),
                        ),
                })
                .on_click(cx.listener(
                    move |this, _, _, cx| match (link_url.as_ref(), chat_id) {
                        (Some(url), Some(chat_id)) => this.queue_link(
                            LinkTarget::Url {
                                url: url.clone(),
                                label: None,
                            },
                            (chat_id.0, message_id.0 as u64),
                            cx,
                        ),
                        _ => this.activate_shared_media_item(action, message_id, cx),
                    },
                ));
            // Telegram Desktop's "Go To Message" on a shared media item.
            list = list.child(if is_message {
                row.context_menu({
                    let owner = cx.entity().downgrade();
                    move |menu, _, _| {
                        let owner = owner.clone();
                        menu.item(PopupMenuItem::new(tr(keys::Q_OPS_GO_TO_MESSAGE)).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.jump_to_shared_media_item_ui(message_id, cx);
                                });
                            },
                        ))
                    }
                })
                .into_any_element()
            } else {
                row.into_any_element()
            });
        }
        if state.can_load_more() {
            list = list.child(
                div().flex().justify_center().py_2().child(
                    Button::new(("shared-media-load-more", tab.index() as u64))
                        .ghost()
                        .label(tr(keys::Q_OPS_LOAD_MORE))
                        .accessibility_label(tr(keys::Q_OPS_LOAD_MORE_SHARED_MEDIA))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_more_shared_media_ui(tab, cx);
                        })),
                ),
            );
        }
        list.into_any_element()
    }

    /// The strip above a list that was reloaded from a picked date, with a
    /// way back to the newest media.
    fn shared_media_date_bar(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let day = session.media.shared_media.tab_at(tab)?.anchor_day?;
        let title =
            quill::search_filters::YearMonth::of(quill::search_filters::local_midnight(day))
                .title();
        Some(
            div()
                .id("shared-media-date-bar")
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr_with(
                    keys::Q_OPS_SHOWING_FROM,
                    &[("title", &title.to_string())],
                ))
                .child(
                    Button::new("shared-media-show-latest")
                        .ghost()
                        .label(tr(keys::Q_OPS_SHOW_LATEST))
                        .accessibility_label(tr(keys::Q_OPS_SHOW_THE_LATEST_SHARED_MEDIA))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.retry_shared_media_ui(cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// The Calendar button of the gallery header; none for tabs without a
    /// calendar (GIFs).
    pub(super) fn shared_media_calendar_button(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        calendar_media(tab)?;
        Some(
            Button::new("shared-media-calendar")
                .icon(gpui_kit::assets::IconName::Calendar)
                .ghost()
                .tooltip(tr(keys::Q_OPS_SHOW_CALENDAR))
                .accessibility_label(tr(keys::Q_OPS_SHOW_CALENDAR))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_shared_media_calendar_ui(cx);
                }))
                .into_any_element(),
        )
    }

    pub(super) fn open_shared_media_calendar_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_shared_media_calendar().is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_COULD_NOT_OPEN_THE_CALENDAR,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_shared_media_calendar(quill::local_time::now_unix());
        }
        cx.notify();
    }
}

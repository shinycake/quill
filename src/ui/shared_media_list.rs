//! The shared-media list with month headers, and the gallery's Calendar
//! button (tdesktop `Info::Media` month sections and its "Show calendar"
//! action): the box marks the days that have media and a picked day reloads
//! the tab from there.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::{SharedMediaTab, calendar_media, month_sections};

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
        let state = &session.media.shared_media.tabs[tab.index()];
        let sections = month_sections(&state.items);
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
            list = list.child(
                div()
                    .id(("shared-media-item", message_id.0 as u64))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Open shared media message {}", message_id.0))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .child(div().text_lg().child(item.glyph))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(item.label.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Photos, videos and GIFs open the viewer over this
                        // list; everything else jumps to the message.
                        if !this.open_shared_media_viewer(message_id, cx) {
                            this.jump_to_shared_media_item_ui(message_id, cx);
                        }
                    }))
                    // Telegram Desktop's "Go To Message" on a shared media
                    // item.
                    .context_menu({
                        let owner = cx.entity().downgrade();
                        move |menu, _, _| {
                            let owner = owner.clone();
                            menu.item(PopupMenuItem::new("Go To Message").on_click(
                                move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.jump_to_shared_media_item_ui(message_id, cx);
                                    });
                                },
                            ))
                        }
                    }),
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
        let day = session.media.shared_media.tabs[tab.index()].anchor_day?;
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
                .child(format!("Showing from {title}"))
                .child(
                    Button::new("shared-media-show-latest")
                        .ghost()
                        .label("Show latest")
                        .accessibility_label("Show the latest shared media")
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
                .tooltip("Show calendar")
                .accessibility_label("Show calendar")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_shared_media_calendar_ui(cx);
                }))
                .into_any_element(),
        )
    }

    pub(super) fn open_shared_media_calendar_ui(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_shared_media_calendar().is_err() {
                self.connection.status_note = "could not open the calendar".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_shared_media_calendar(quill::local_time::now_unix());
        }
        cx.notify();
    }
}

//! The shared-media list with month headers, and the gallery's Calendar
//! button (tdesktop `Info::Media` month sections and its "Show calendar"
//! action): the box marks the days that have media and a picked day reloads
//! the tab from there.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::scroll_load::{load_due, loading_row, scrolled_near_end, watch_scroll};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
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
        let selecting = !session.media.shared_media.selection.is_empty();
        let mut list = div()
            .id(("shared-media-items", tab.index() as u64))
            .flex_1()
            .overflow_y_scroll()
            .when_some(self.shared_media_grid.list_scroll(tab), |list, scroll| {
                list.track_scroll(scroll)
            })
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
            let track = SharedTrack::of(item);
            let item_ix = if is_message {
                message_id.0 as u64
            } else {
                index as u64
            };
            let row = div()
                .id(("shared-media-item", item_ix))
                .debug_selector(move || format!("shared-media-item-{item_ix}"))
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
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.shared_media_selecting() && is_message {
                        this.toggle_shared_media_selection(message_id, cx);
                        return;
                    }
                    match (link_url.as_ref(), chat_id) {
                        // Music and voice rows play in place (tdesktop
                        // `Overview::Layout::{Document, Voice}`).
                        (None, Some(chat_id)) if track.is_some() => {
                            if let Some(track) = track {
                                this.press_shared_track(chat_id, message_id, track, cx);
                            }
                        }
                        (Some(url), Some(chat_id)) => this.queue_link(
                            LinkTarget::Url {
                                url: url.clone(),
                                label: None,
                            },
                            (chat_id.0, message_id.0 as u64),
                            cx,
                        ),
                        _ => this.activate_shared_media_item(action, message_id, cx),
                    }
                }));
            // Telegram Desktop's row menu: Go to message, Forward, Delete, Select.
            list = list.child(if is_message {
                let selected = self.shared_media_is_selected(message_id);
                row.on_hover(self.shared_media_hover_listener(message_id, cx))
                    .when(selected, |this| this.bg(cx.theme().accent))
                    .when(selecting, |this| {
                        this.child(self.shared_media_check(selected, cx))
                    })
                    .context_menu(self.shared_media_context_menu(item, cx))
                    .into_any_element()
            } else {
                row.into_any_element()
            });
        }
        if state.loading_more {
            list = list.child(loading_row(
                ("shared-media-loading", tab.index() as u64),
                cx,
            ));
        }
        list.on_scroll_wheel(watch_scroll(cx, Self::shared_media_more))
            .into_any_element()
    }

    /// The next page of the open list tab, once it is scrolled near its end
    /// or does not fill the panel. A failed page is not retried on its own.
    fn shared_media_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(media) = self.session().map(|s| &s.media.shared_media) else {
            return;
        };
        let tab = media.active_tab;
        let Some(state) = media.tab_at(tab) else {
            return;
        };
        let (Some(scroll), Some(asked)) = (
            self.shared_media_grid.list_scroll(tab),
            self.shared_media_grid.list_asked(tab),
        ) else {
            return;
        };
        if !media.open
            || !load_due(
                scrolled_near_end(scroll),
                state.can_load_more(),
                state.loading_more,
            )
            || !asked.allow(state.items.len(), scrolled)
        {
            return;
        }
        self.load_more_shared_media_ui(tab, cx);
    }

    /// The strip above a list that was reloaded from a picked date, with a
    /// way back to the newest media.
    pub(super) fn shared_media_date_bar(
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

/// A Music or Voice row's track: what a press on the row plays.
#[derive(Clone, Copy)]
pub(super) struct SharedTrack {
    file: quill::ids::FileId,
    voice: bool,
    duration: f64,
}

impl SharedTrack {
    pub(super) fn of(item: &quill::state::SharedMediaItem) -> Option<Self> {
        use quill::state::SharedMediaMeta;
        let (voice, duration) = match &item.meta {
            SharedMediaMeta::Music { duration, .. } => (false, *duration),
            SharedMediaMeta::Voice { duration, .. } => (true, *duration),
            _ => return None,
        };
        (item.file.0 != 0).then_some(Self {
            file: item.file,
            voice,
            duration: f64::from(duration),
        })
    }
}

impl QuillApp {
    fn press_shared_track(
        &mut self,
        chat: ChatId,
        message: MessageId,
        track: SharedTrack,
        cx: &mut Context<Self>,
    ) {
        let SharedTrack {
            file,
            voice,
            duration,
        } = track;
        self.press_media(file, cx, |this, cx| {
            if voice {
                this.toggle_voice_playback(chat, message, file, false, duration, cx);
            } else {
                this.toggle_audio_playback(chat, message, file, duration, cx);
            }
        });
    }
}

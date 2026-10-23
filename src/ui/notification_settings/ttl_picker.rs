//! The auto-delete timer picker below the conversation header.
//!
//! Regular chats follow tdesktop `menu/menu_ttl.cpp` (`TTLBox`): one pick
//! from "Off" and a fixed list of 16 periods
//! (`quill::auto_delete::TTL_CHOICES`), and a Save button
//! (`setChatMessageAutoDeleteTime`, schema 1.8.67 line 13454). tdesktop
//! draws that pick as a drum picker; the kit has none, so it is a `Slider`
//! over the list index, step 0 being Off. Secret chats keep their own self-destruct presets
//! (any number of seconds is valid there).

use super::*;
use crate::ui::index_sliders::StepSlider;
use quill::auto_delete::{TTL_CHOICES, format_ttl, nearest_choice_index};
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Open the picker for `chat_id`, the slider on the chat's current
    /// timer (the nearest period), or on Off when none is set.
    pub(in crate::ui) fn open_ttl_picker(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let current = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map_or(0, |chat| chat.message_auto_delete_time);
        let start = if current > 0 {
            nearest_choice_index(current) + 1
        } else {
            0
        };
        self.set_step_slider(StepSlider::AutoDelete, start, cx);
        self.notify.ttl_picker_open = true;
        cx.notify();
    }

    /// The period the slider is on; 0 is Off.
    fn picked_ttl(&self, cx: &App) -> i32 {
        self.settings
            .index_sliders
            .index(StepSlider::AutoDelete, cx)
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| TTL_CHOICES.get(index).copied())
            .unwrap_or(0)
    }

    pub(in crate::ui) fn ttl_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let is_secret =
            open_chat_summary.is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let current = open_chat_summary
            .and_then(|chat| chat.ttl_status_line())
            .unwrap_or_else(|| tr(keys::SETTINGS_TTL_AFTER_OFF).to_string());
        let title = if is_secret {
            tr(keys::SETTINGS_TTL_AFTER_SUBTITLE)
        } else {
            tr(keys::Q_PREFS_AUTO_DELETE_TIMER)
        };
        let about = if is_secret {
            tr(keys::Q_PREFS_MESSAGES_AUTO_DELETE_AFTER_THE).to_string()
        } else {
            open_chat_summary
                .map(|chat| quill::auto_delete::about_line(&chat.kind).to_string())
                .unwrap_or_default()
        };
        let body = if is_secret {
            self.secret_ttl_presets(open_chat, open_chat_summary, cx)
        } else {
            self.regular_ttl_picker(open_chat, cx)
        };
        div()
            .id("ttl-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("close-ttl-picker")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip(tr(keys::CLOSE))
                            .accessibility_label(tr(keys::CLOSE))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.ttl_picker_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::Q_PREFS_CURRENT,
                        &[
                            ("current", &current.to_string()),
                            ("about", &about.to_string()),
                        ],
                    )),
            )
            .child(body)
    }

    /// `TTLBox`: the slider (Off first), Cancel, Save.
    fn regular_ttl_picker(&self, open_chat: Option<ChatId>, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.picked_ttl(cx);
        let picked_label = if picked == 0 {
            tr(keys::SETTINGS_TTL_AFTER_OFF).to_string()
        } else {
            format_ttl(picked)
        };
        let slider = self.step_slider_row(
            StepSlider::AutoDelete,
            "ttl-slider",
            tr(keys::Q_PREFS_AUTO_DELETE_TIMER).to_string(),
            picked_label,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(slider)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1())
                    .child(
                        Button::new("ttl-cancel")
                            .small()
                            .ghost()
                            .label(tr(keys::CANCEL))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.ttl_picker_open = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ttl-save")
                            .small()
                            .primary()
                            .label(tr(keys::SETTINGS_SAVE))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(chat_id) = open_chat {
                                    let secs = this.picked_ttl(cx);
                                    this.apply_chat_ttl(chat_id, secs, cx);
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Secret chats: Off and a handful of self-destruct presets.
    fn secret_ttl_presets(
        &self,
        open_chat: Option<ChatId>,
        open_chat_summary: Option<&ChatSummary>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let presets: Vec<(&str, i32)> = vec![
            (tr(keys::SETTINGS_TTL_AFTER_OFF), 0),
            ("5s", 5),
            ("30s", 30),
            ("1m", 60),
            ("1h", 3600),
            ("1d", 86400),
            ("1w", 604800),
        ];
        let active_ix = presets.iter().position(|(_, secs)| {
            open_chat_summary.is_some_and(|chat| chat.message_auto_delete_time == *secs)
        });
        RadioGroup::horizontal("ttl-presets")
            .selected_index(active_ix)
            .children(
                presets
                    .iter()
                    .map(|(label, secs)| Radio::new(format!("ttl-set-{secs}")).label(*label)),
            )
            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                let Some(secs) = presets.get(ix).map(|preset| preset.1) else {
                    return;
                };
                if let Some(chat_id) = open_chat {
                    this.apply_chat_ttl(chat_id, secs, cx);
                }
            }))
            .into_any_element()
    }
}

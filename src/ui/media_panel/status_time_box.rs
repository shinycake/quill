//! The "Set Status for..." box (tdesktop `PickUntilBox` over
//! `Ui::TimePickerBox`): a drum of durations with the chosen one between
//! two lines, Set Status and Cancel. The wheel, a click and the arrow keys
//! move the drum. The kit has no drum picker, so this is drawn here.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use quill::emoji_status_selector::{format_mute_for, picker_start_index, time_picker_values};
use std::cell::RefCell;
use std::rc::Rc;

/// `st::historyMessagesTTLPickerHeight` and `...ItemHeight`.
const DRUM_HEIGHT: f32 = 200.;
const ITEM_HEIGHT: f32 = 40.;
/// Rows each side of the chosen one.
const SIDE_ROWS: isize = 2;

/// The box while it is open: the status it sets and the drum's row.
pub(in crate::ui) struct StatusUntil {
    id: i64,
    pub(super) index: usize,
    /// Wheel movement not yet worth a row.
    wheel: f32,
    focus: FocusHandle,
}

impl QuillApp {
    pub(super) fn open_status_time_box(
        &mut self,
        id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.pickers.media_panel.until = Some(StatusUntil {
            id,
            index: picker_start_index(&time_picker_values(), 0),
            wheel: 0.,
            focus,
        });
        cx.notify();
    }

    fn close_status_time_box(&mut self, cx: &mut Context<Self>) {
        self.pickers.media_panel.until = None;
        cx.notify();
    }

    /// Move the drum `rows` rows, clamped to the list.
    pub(super) fn step_status_drum(&mut self, rows: isize, cx: &mut Context<Self>) {
        if let Some(until) = self.pickers.media_panel.until.as_mut() {
            let last = time_picker_values().len().saturating_sub(1);
            until.index = until.index.saturating_add_signed(rows).min(last);
            cx.notify();
        }
    }

    fn set_status_drum(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(until) = self.pickers.media_panel.until.as_mut() {
            until.index = index.min(time_picker_values().len().saturating_sub(1));
            cx.notify();
        }
    }

    /// Set Status: the chosen duration from now.
    pub(super) fn confirm_status_time(&mut self, cx: &mut Context<Self>) {
        let Some(until) = self.pickers.media_panel.until.take() else {
            return;
        };
        let secs = time_picker_values().get(until.index).copied().unwrap_or(0);
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        match quill::emoji_status_selector::pick(Some(until.id), secs, premium) {
            quill::emoji_status_selector::StatusPick::Set { id, secs } => {
                self.change_emoji_status_for(Some(id), secs, cx)
            }
            _ => self.open_premium_at(quill::premium_hub::PremiumFeature::EmojiStatus, cx),
        }
        cx.notify();
    }

    fn status_drum(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(until) = self.pickers.media_panel.until.as_ref() else {
            return div().into_any_element();
        };
        let values = time_picker_values();
        let chosen = until.index;
        let mut items = div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center();
        for offset in -SIDE_ROWS..=SIDE_ROWS {
            let row = chosen.checked_add_signed(offset);
            let value = row.and_then(|row| values.get(row).map(|secs| (row, *secs)));
            let Some((row, secs)) = value else {
                items = items.child(div().h(px(ITEM_HEIGHT)));
                continue;
            };
            let near = offset == 0;
            items = items.child(
                div()
                    .id(("status-drum-row", row))
                    .h(px(ITEM_HEIGHT))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(cx.theme().foreground)
                    .opacity(match offset.abs() {
                        0 => 1.,
                        1 => 0.7,
                        _ => 0.45,
                    })
                    .when(near, |this| this.font_semibold())
                    .role(gpui_kit::Role::Button)
                    .aria_selected(near)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_status_drum(row, cx)))
                    .child(format_mute_for(secs)),
            );
        }
        let line = |top: f32| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(top))
                .h(px(1.))
                .bg(cx.theme().primary)
        };
        let middle = (DRUM_HEIGHT - ITEM_HEIGHT) / 2.;
        div()
            .id("status-drum")
            .relative()
            .w_full()
            .h(px(DRUM_HEIGHT))
            .overflow_hidden()
            .role(gpui_kit::Role::Group)
            .aria_label(tr(keys::Q_EMOJI_STATUS_TIME_PICKER))
            .track_focus(&until.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "up" => this.step_status_drum(-1, cx),
                    "down" => this.step_status_drum(1, cx),
                    "pageup" => this.step_status_drum(-SIDE_ROWS, cx),
                    "pagedown" => this.step_status_drum(SIDE_ROWS, cx),
                    "home" => this.set_status_drum(0, cx),
                    "end" => this.set_status_drum(usize::MAX, cx),
                    _ => {}
                }
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let moved = event.delta.pixel_delta(px(ITEM_HEIGHT)).y;
                let Some(until) = this.pickers.media_panel.until.as_mut() else {
                    return;
                };
                until.wheel -= f32::from(moved);
                let rows = (until.wheel / ITEM_HEIGHT).trunc();
                until.wheel -= rows * ITEM_HEIGHT;
                if rows != 0. {
                    this.step_status_drum(rows as isize, cx);
                }
            }))
            .child(items)
            .child(line(middle))
            .child(line(middle + ITEM_HEIGHT))
            .into_any_element()
    }

    pub(in crate::ui) fn build_status_time_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EmojiStatusUntil, |this, _, cx| {
                this.close_status_time_box(cx);
            });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(this.status_drum(cx))));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("status-until-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_status_time_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::EmojiStatusUntil, window, cx);
                        })),
                )
                .child(
                    Button::new("status-until-set")
                        .label(tr(keys::EMOJI_STATUS_FOR_SUBMIT))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_status_time(cx);
                            this.close_kit_dialog_if_done(DialogKind::EmojiStatusUntil, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::EMOJI_STATUS_FOR_TITLE,
                )))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// tdesktop's "Set Status for..." box.
    EmojiStatusUntil => DialogSpec::new(
        5150,
        |app| app.pickers.media_panel.until.is_some(),
        QuillApp::build_status_time_dialog,
    ),
}

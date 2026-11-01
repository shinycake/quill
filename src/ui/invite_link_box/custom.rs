//! The Custom boxes over the link box: tdesktop's `ChooseDateTimeBox`
//! titled "Expire after", and its number box for the member limit. Each
//! shows in place of the link box and returns to it.
#![deny(clippy::indexing_slicing)]

use super::CustomBox;
use crate::ui::app::QuillApp;
use crate::ui::scheduled::{new_date_time_picker, picked_unix};
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::button::*;
use gpui_kit::component::date_picker::{DatePicker, DatePickerEvent};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::invite_link_form::{
    CUSTOM_DATE_MAX_AHEAD, DAY, Expire, Usage, custom_date_ok, custom_limit,
};
use quill::lang::{keys, tr};

impl QuillApp {
    fn invite_link_custom(&self) -> Option<&CustomBox> {
        self.invite_link_box()?.custom.as_ref()
    }

    fn set_invite_link_custom(&mut self, custom: Option<CustomBox>, cx: &mut Context<Self>) {
        if let Some(link_box) = self.admin.invite_link_box.as_mut() {
            link_box.custom = custom;
        }
        cx.notify();
    }

    /// tdesktop `expireGroup` Custom: the picker opens on the chosen date,
    /// or a day from now.
    pub(super) fn open_invite_link_custom_date(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(link_box) = self.invite_link_box() else {
            return;
        };
        let now = WallClock::of(cx).now();
        let initial = match link_box.form.expire {
            Expire::At(date) if i64::from(date) > now => i64::from(date),
            Expire::After(secs) => now.saturating_add(i64::from(secs)),
            _ => now.saturating_add(i64::from(DAY)),
        };
        let picker = new_date_time_picker(window, cx, initial, CUSTOM_DATE_MAX_AHEAD);
        cx.subscribe(&picker.state, |_, _, _: &DatePickerEvent, cx| cx.notify())
            .detach();
        self.set_invite_link_custom(Some(CustomBox::Date(picker)), cx);
    }

    /// tdesktop `usageGroup` Custom: a number field holding the current
    /// limit.
    pub(super) fn open_invite_link_custom_limit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(link_box) = self.invite_link_box() else {
            return;
        };
        let current = match link_box.form.usage {
            Usage::Limit(count) => count.to_string(),
            Usage::Any => String::new(),
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::GROUP_INVITE_CUSTOM_LIMIT))
                .validate(|text, _| text.len() <= 5 && text.chars().all(|c| c.is_ascii_digit()))
                .default_value(current)
        });
        cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    this.save_invite_link_custom(cx);
                    this.close_kit_dialog_if_done(DialogKind::InviteLinkLimit, window, cx);
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        if let Some(link_box) = self.admin.invite_link_box.as_mut() {
            link_box.focus_custom = true;
        }
        self.set_invite_link_custom(Some(CustomBox::Limit(input)), cx);
    }

    /// The Custom box's value, when it is one tdesktop takes.
    fn invite_link_custom_value(&self, cx: &App) -> Option<CustomValue> {
        match self.invite_link_custom()? {
            CustomBox::Date(picker) => picked_unix(picker, cx)
                .filter(|date| custom_date_ok(*date, WallClock::of(cx).now()))
                .and_then(|date| i32::try_from(date).ok())
                .map(CustomValue::Date),
            CustomBox::Limit(input) => {
                custom_limit(&input.read(cx).value()).map(CustomValue::Limit)
            }
        }
    }

    fn save_invite_link_custom(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.invite_link_custom_value(cx) else {
            return;
        };
        if let Some(link_box) = self.admin.invite_link_box.as_mut() {
            match value {
                CustomValue::Date(date) => link_box.form.expire = Expire::At(date),
                CustomValue::Limit(count) => link_box.form.usage = Usage::Limit(count),
            }
        }
        self.set_invite_link_custom(None, cx);
    }

    /// The number field, once: the kit focuses a dialog as it opens, so
    /// the field takes focus after the box's first frame.
    fn take_invite_link_custom_focus(&mut self) -> Option<Entity<InputState>> {
        let link_box = self.admin.invite_link_box.as_mut()?;
        if !std::mem::take(&mut link_box.focus_custom) {
            return None;
        }
        match &link_box.custom {
            Some(CustomBox::Limit(input)) => Some(input.clone()),
            _ => None,
        }
    }

    fn invite_link_custom_open(&self, date: bool) -> bool {
        self.invite_link_custom()
            .is_some_and(|custom| custom.is_date() == date)
    }

    fn build_invite_link_custom_dialog(
        kind: DialogKind,
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, kind, |this, _, cx| {
            this.set_invite_link_custom(None, cx);
        });
        app.update(cx, |this, cx| {
            let date = kind == DialogKind::InviteLinkExpire;
            let title = if date {
                tr(keys::GROUP_INVITE_EXPIRE_AFTER)
            } else {
                tr(keys::GROUP_INVITE_USAGE_TITLE)
            };
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            let field = match this.invite_link_custom() {
                Some(CustomBox::Date(picker)) => DatePicker::new(&picker.state)
                    .placeholder(tr(keys::GROUP_INVITE_EXPIRE_AFTER))
                    .into_any_element(),
                Some(CustomBox::Limit(input)) => Input::new(input).into_any_element(),
                None => return dialog.on_close(on_close),
            };
            let valid = this.invite_link_custom_value(cx).is_some();
            let focus = this.take_invite_link_custom_focus();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("invite-link-custom-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_invite_link_custom(None, cx);
                            this.close_kit_dialog_if_done(kind, window, cx);
                        })),
                )
                .child(
                    Button::new("invite-link-custom-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .disabled(!valid)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.save_invite_link_custom(cx);
                            this.close_kit_dialog_if_done(kind, window, cx);
                        })),
                );
            let field = std::rc::Rc::new(std::cell::RefCell::new(Some(field)));
            let focus = std::rc::Rc::new(std::cell::RefCell::new(focus));
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, window, cx| {
                        if let Some(input) = focus.borrow_mut().take() {
                            window.defer(cx, move |window, cx| {
                                input.update(cx, |input, cx| input.focus(window, cx));
                            });
                        }
                        let field = field
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(div().py_2().child(field))
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

enum CustomValue {
    Date(i32),
    Limit(i32),
}

crate::ui::shell::register_dialogs! {
    /// "Custom" of "Limit by time period": the date and time picker.
    InviteLinkExpire => DialogSpec::new(
        4860,
        |app| app.invite_link_custom_open(true),
        |app, shell, dialog, cx| {
            QuillApp::build_invite_link_custom_dialog(
                DialogKind::InviteLinkExpire,
                app,
                shell,
                dialog,
                cx,
            )
        },
    ),
    /// "Custom" of "Limit by number of users".
    InviteLinkLimit => DialogSpec::new(
        4862,
        |app| app.invite_link_custom_open(false),
        |app, shell, dialog, cx| {
            QuillApp::build_invite_link_custom_dialog(
                DialogKind::InviteLinkLimit,
                app,
                shell,
                dialog,
                cx,
            )
        },
    ),
}

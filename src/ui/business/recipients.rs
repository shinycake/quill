//! The recipients section of the greeting, away and chatbot screens and its chat
//! picker (tdesktop `settings_recipients_helper.cpp` and the business mode
//! of `edit_filter_chats_list.cpp`: private chats with users, not
//! yourself, up to 100, plus the chat types).

use super::{PickerDraft, Recipients, View};
use crate::ui::app::QuillApp;
use crate::ui::chat_row::chat_avatar;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_bots::{bot_set_excluded, bot_set_included};
use quill::business_settings::{BusinessChats, BusinessRecipients, ChatType, RECIPIENT_CHATS_MAX};
use quill::folder_picker::{filter_chats, picker_counter};
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::ChatKind;

const AVATAR: f32 = 28.;

fn target_key(target: Recipients) -> &'static str {
    match target {
        Recipients::Greeting => "greeting",
        Recipients::Away => "away",
        Recipients::Bot => "bot",
    }
}

fn type_key(kind: ChatType) -> &'static str {
    match kind {
        ChatType::NewChats => "new",
        ChatType::ExistingChats => "existing",
        ChatType::Contacts => "contacts",
        ChatType::NonContacts => "non-contacts",
    }
}

impl QuillApp {
    pub(super) fn business_recipients(&self, target: Recipients) -> Option<&BusinessRecipients> {
        let d = self.dialogs.business_dialog.as_ref()?;
        match target {
            Recipients::Greeting => d.greeting.as_ref().map(|g| &g.recipients),
            Recipients::Away => d.away.as_ref().map(|a| &a.recipients),
            Recipients::Bot => d.chatbot.as_ref().map(|b| &b.recipients),
        }
    }

    fn business_recipients_mut(&mut self, target: Recipients) -> Option<&mut BusinessRecipients> {
        let d = self.business_mut()?;
        match target {
            Recipients::Greeting => d.greeting.as_mut().map(|g| &mut g.recipients),
            Recipients::Away => d.away.as_mut().map(|a| &mut a.recipients),
            Recipients::Bot => d.chatbot.as_mut().map(|b| &mut b.recipients),
        }
    }

    /// Private chats with other users, A to Z: what the picker offers.
    pub(super) fn business_picker_chats(&self) -> Vec<(i64, String)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let me = session.my_user_id;
        let mut chats: Vec<(i64, String)> = session
            .chats
            .values()
            .filter(|c| matches!(c.kind, ChatKind::Private { user_id } if Some(user_id.0) != me))
            .map(|c| (c.id.0, c.title.clone()))
            .collect();
        chats.sort_by_key(|c| c.1.to_lowercase());
        chats
    }

    fn business_chat_title(&self, chat_id: i64) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .map_or_else(
                || tr_with(keys::Q_OPS_CHAT, &[("chat_id", &chat_id.to_string())]),
                |c| c.title.clone(),
            )
    }

    pub(super) fn business_avatar(&self, chat_id: i64, title: &str) -> AnyElement {
        let roots = self.media_display_roots();
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(quill::ids::ChatId(chat_id)))
            .and_then(|path| sandboxed_display_path(path, &roots));
        chat_avatar(title, photo.as_deref(), AVATAR).into_any_element()
    }

    /// Open the include or exclude picker on a copy of the current list.
    pub(super) fn open_business_picker(
        &mut self,
        target: Recipients,
        include: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.business_recipients(target) else {
            return;
        };
        let chats = if include {
            current.included.clone()
        } else {
            current.excluded.clone()
        };
        let search = super::live_input(window, cx, tr(keys::Q_OPS_SEARCH));
        if let Some(d) = self.business_mut() {
            d.picker = Some(PickerDraft {
                target,
                include,
                chats,
                search,
            });
        }
        self.business_go(View::Picker, window, cx);
    }

    /// Whether the picker offers chat types: always, except a bot's
    /// exclusions in "Only Selected Chats" (tdesktop `usersOnly`).
    pub(super) fn business_picker_types(&self) -> bool {
        let Some(picker) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.picker.as_ref())
        else {
            return true;
        };
        picker.include
            || picker.target != Recipients::Bot
            || self
                .business_recipients(Recipients::Bot)
                .is_some_and(|r| r.all_but_excluded)
    }

    /// Done in the picker: tdesktop's save rules, then back.
    pub(super) fn finish_business_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.business_mut().and_then(|d| d.picker.take()) else {
            return;
        };
        let target = picker.target;
        if let Some(recipients) = self.business_recipients_mut(target) {
            match (target, picker.include) {
                (Recipients::Bot, true) => bot_set_included(recipients, picker.chats),
                (Recipients::Bot, false) => bot_set_excluded(recipients, picker.chats),
                (_, true) => recipients.set_included(picker.chats),
                (_, false) => recipients.set_excluded(picker.chats),
            }
        }
        let back = View::Picker.parent(Some(target));
        self.business_go(back, window, cx);
    }

    pub(super) fn remove_business_entry(
        &mut self,
        target: Recipients,
        excluded: bool,
        chat: Option<i64>,
        chat_type: Option<ChatType>,
        cx: &mut Context<Self>,
    ) {
        if let Some(r) = self.business_recipients_mut(target) {
            let list = if excluded {
                &mut r.excluded
            } else {
                &mut r.included
            };
            if let Some(chat) = chat {
                list.chat_ids.retain(|id| *id != chat);
            }
            if let Some(kind) = chat_type {
                list.types.set(kind, false);
            }
            // tdesktop: an emptied inclusion falls back to "all except".
            if !r.all_but_excluded && r.included.is_empty() {
                r.all_but_excluded = true;
            }
        }
        cx.notify();
    }

    /// One list of the section: its title, the add button and the chosen
    /// types and chats.
    fn business_list_block(
        &self,
        target: Recipients,
        excluded: bool,
        list: BusinessChats,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let key = target_key(target);
        let side = if excluded { "excluded" } else { "included" };
        let (title, button) = if excluded {
            (
                tr(keys::Q_OPS_EXCLUDED_CHATS),
                tr(keys::Q_OPS_EXCLUDE_CHATS),
            )
        } else {
            (tr(keys::Q_OPS_INCLUDED_CHATS), tr(keys::Q_OPS_SELECT_CHATS))
        };
        let mut block = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .pt_2()
                    .text_xs()
                    .font_semibold()
                    .text_color(muted)
                    .child(title),
            )
            .child(
                div().flex().child(
                    Button::new(SharedString::from(format!("business-{key}-pick-{side}")))
                        .label(button)
                        .icon(if excluded {
                            IconName::Minus
                        } else {
                            IconName::Plus
                        })
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_business_picker(target, !excluded, window, cx);
                        })),
                ),
            );
        for kind in list.types.chosen() {
            block = block.child(self.business_entry_row(
                target,
                excluded,
                format!("business-{key}-{side}-type-{}", type_key(kind)),
                kind.label().to_string(),
                None,
                Some(kind),
                cx,
            ));
        }
        for chat_id in list.chat_ids {
            let title = self.business_chat_title(chat_id);
            block = block.child(self.business_entry_row(
                target,
                excluded,
                format!("business-{key}-{side}-chat-{chat_id}"),
                title,
                Some(chat_id),
                None,
                cx,
            ));
        }
        block
    }

    /// "Recipients" (or the bot's "Chats accessible for the bot"): the two
    /// radios and the chosen chats and types.
    pub(super) fn business_recipients_section(
        &self,
        target: Recipients,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(recipients) = self.business_recipients(target).cloned() else {
            return div().into_any_element();
        };
        let key = target_key(target);
        let all = recipients.all_but_excluded;
        let (title, about) = match target {
            Recipients::Greeting => (tr(keys::AWAY_RECIPIENTS), tr(keys::GREETING_SELECT)),
            Recipients::Away => (tr(keys::AWAY_RECIPIENTS), tr(keys::AWAY_SELECT)),
            Recipients::Bot => (
                tr(keys::CHATBOTS_ACCESS_TITLE),
                tr(keys::CHATBOTS_EXCLUDE_ABOUT),
            ),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().font_semibold().child(title))
            .child(
                RadioGroup::vertical(SharedString::from(format!("business-{key}-recipients")))
                    .selected_index(Some(if all { 0 } else { 1 }))
                    .child(
                        Radio::new(SharedString::from(format!("business-{key}-all")))
                            .label(tr(keys::CHATBOTS_ALL_EXCEPT)),
                    )
                    .child(
                        Radio::new(SharedString::from(format!("business-{key}-selected")))
                            .label(tr(keys::Q_OPS_ONLY_SELECTED_CHATS)),
                    )
                    .on_click(cx.listener(move |this, &ix: &usize, window, cx| {
                        let included_empty = this
                            .business_recipients(target)
                            .is_some_and(|r| r.included.is_empty());
                        // tdesktop: "Only Selected" with nothing selected
                        // opens the picker and stays on "all except".
                        if ix == 1 && included_empty {
                            this.open_business_picker(target, true, window, cx);
                            return;
                        }
                        if let Some(r) = this.business_recipients_mut(target) {
                            r.all_but_excluded = ix == 0;
                        }
                        cx.notify();
                    })),
            );
        if !all {
            section = section.child(self.business_list_block(
                target,
                false,
                recipients.included.clone(),
                cx,
            ));
        }
        // A bot's exclusions show in both modes.
        if all || target == Recipients::Bot {
            section = section.child(self.business_list_block(
                target,
                true,
                recipients.excluded.clone(),
                cx,
            ));
        }
        section
            .child(div().text_xs().text_color(muted).child(about))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn business_entry_row(
        &self,
        target: Recipients,
        excluded: bool,
        id: String,
        title: String,
        chat: Option<i64>,
        chat_type: Option<ChatType>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let avatar = match chat {
            Some(chat_id) => self.business_avatar(chat_id, &title),
            None => div()
                .size(px(AVATAR))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(cx.theme().secondary)
                .child(Icon::new(IconName::Users).size(px(14.)))
                .into_any_element(),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .py_0p5()
            .child(avatar)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .child(crate::ui::bidi_line::one_line_plain(title.clone())),
            )
            .child(
                Button::new(SharedString::from(id))
                    .icon(IconName::X)
                    .ghost()
                    .small()
                    .tooltip(tr(keys::BOX_REMOVE))
                    .accessibility_label(tr_with(
                        keys::Q_OPS_REMOVE,
                        &[("title", &title.to_string())],
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_business_entry(target, excluded, chat, chat_type, cx);
                    })),
            )
            .into_any_element()
    }

    /// The include / exclude picker: chat types, search and chats.
    pub(super) fn business_picker_body(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(picker) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.picker.as_ref())
        else {
            return div();
        };
        let chosen: BusinessChats = picker.chats.clone();
        let query = picker.search.read(cx).value().to_string();
        let mut types = div().flex().flex_col().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(muted)
                .child(tr(keys::Q_OPS_CHAT_TYPES)),
        );
        for kind in ChatType::ALL {
            types = types.child(
                Checkbox::new(SharedString::from(format!(
                    "business-pick-type-{}",
                    type_key(kind)
                )))
                .checked(chosen.types.has(kind))
                .label(kind.label())
                .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                    if let Some(p) = this.business_mut().and_then(|d| d.picker.as_mut()) {
                        p.chats.types.set(kind, on);
                    }
                    cx.notify();
                })),
            );
        }
        let rows = filter_chats(&self.business_picker_chats(), &query);
        let mut list = div()
            .id("business-picker-list")
            .flex()
            .flex_col()
            .gap_0p5()
            .overflow_y_scroll()
            .max_h(px(280.));
        if rows.is_empty() {
            list = list.child(
                div()
                    .py_3()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_OPS_NO_CHATS_FOUND_2)),
            );
        }
        for (chat_id, title) in rows {
            let ticked = chosen.chat_ids.contains(&chat_id);
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_0p5()
                    .child(self.business_avatar(chat_id, &title))
                    .child(
                        Checkbox::new(("business-pick-chat", chat_id as u64))
                            .checked(ticked)
                            .label(title)
                            .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                                if on == ticked {
                                    return;
                                }
                                let added = this
                                    .business_mut()
                                    .and_then(|d| d.picker.as_mut())
                                    .is_some_and(|p| p.chats.toggle_chat(chat_id));
                                if !added {
                                    this.business_error(
                                        tr_with(
                                            keys::Q_LEFTOVERS_SELECT_UP_TO_CHATS,
                                            &[("count", &RECIPIENT_CHATS_MAX.to_string())],
                                        ),
                                        cx,
                                    );
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let show_types = self.business_picker_types();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .when(show_types, |this| this.child(types))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(tr(keys::RECENT_CHATS)),
                    )
                    .child(
                        div()
                            .id("business-picker-counter")
                            .text_xs()
                            .text_color(muted)
                            .child(picker_counter(
                                chosen.chat_ids.len(),
                                RECIPIENT_CHATS_MAX as i32,
                            )),
                    ),
            )
            .child(Input::new(&picker.search).aria_label(tr(keys::Q_OPS_SEARCH_CHATS)))
            .child(list)
    }
}

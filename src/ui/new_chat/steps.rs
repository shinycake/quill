//! What each step of New Group / New Channel shows: the title row, the
//! body and the buttons.
#![deny(clippy::indexing_slicing)]

use super::NewChatDialog;
use crate::ui::app::QuillApp;
use crate::ui::group_edit_box::photo::PHOTO_PX;
use crate::ui::profile_photo_sources::PhotoTarget;
use crate::ui::shell::{DialogKind, dialog_title};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::form::{field, v_form};
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::select::Select;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::group_info_edit::DESCRIPTION_MAX;
use quill::ids::ChatId;
use quill::lang::{Key, keys, tr, tr_with};
use quill::new_chat::{CreateFailure, NewChatKind, NewChatStep, NewChatView, members_counter};
use quill::telegram::envelope::ChatKind;

/// The dialog's name for the kind.
fn kind_title(kind: NewChatKind) -> Key {
    match kind {
        NewChatKind::Group => keys::CREATE_GROUP_TITLE,
        NewChatKind::Megagroup => keys::Q_OPS_NEW_SUPERGROUP,
        NewChatKind::Channel => keys::CREATE_CHANNEL_TITLE,
    }
}

/// A refused create, as tdesktop words it; `None` when the name field or
/// the limit box says it.
fn failure_text(failure: CreateFailure, reason: &str) -> Option<String> {
    match failure {
        CreateFailure::NoTitle | CreateFailure::TooManyChannels => None,
        CreateFailure::Other => Some(tr_with(keys::Q_NEW_CHAT_FAILED, &[("reason", reason)])),
        known => known
            .message()
            .map(|key| tr(key).replace("{more_info}", "").trim().to_string()),
    }
}

impl QuillApp {
    pub(super) fn new_chat_title(&self, view: &NewChatView, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.new_chat_dialog() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        match view.step {
            NewChatStep::Info if dialog.kind.has_auto_delete_menu() => h_flex()
                .w_full()
                .pr_8()
                .items_center()
                .justify_between()
                .gap_2()
                .child(dialog_title(tr(kind_title(dialog.kind))))
                .child(self.new_chat_menu(dialog, cx))
                .into_any_element(),
            NewChatStep::Info | NewChatStep::Done(_) => {
                dialog_title(tr(kind_title(dialog.kind))).into_any_element()
            }
            NewChatStep::AutoDelete => {
                dialog_title(tr(keys::MANAGE_MESSAGES_TTL_TITLE)).into_any_element()
            }
            NewChatStep::Type(_) => dialog_title(tr(if dialog.kind == NewChatKind::Channel {
                keys::MANAGE_PEER_CHANNEL_TYPE
            } else {
                keys::MANAGE_PEER_GROUP_TYPE
            }))
            .into_any_element(),
            NewChatStep::Members | NewChatStep::Subscribers(_) => {
                let already_in = match view.step {
                    NewChatStep::Subscribers(chat) => self
                        .session()
                        .and_then(|s| match s.chats.get(&chat.0)?.kind {
                            ChatKind::Supergroup { supergroup_id, .. } => s
                                .groups
                                .supergroup_member_counts
                                .get(&supergroup_id)
                                .copied(),
                            _ => None,
                        })
                        .unwrap_or(1),
                    _ => 1,
                };
                let size_max = self.session().map_or(0, |s| s.groups.supergroup_size_max);
                let counter = members_counter(
                    dialog.kind,
                    already_in,
                    dialog.selected_users.len(),
                    size_max,
                );
                h_flex()
                    .items_baseline()
                    .gap_2()
                    .child(dialog_title(tr(keys::PROFILE_ADD_PARTICIPANT)))
                    .children(counter.map(|text| div().text_sm().text_color(muted).child(text)))
                    .into_any_element()
            }
        }
    }

    /// The group's top bar menu: "Auto-Delete" with the period it will
    /// have (tdesktop `addTopButton(st::infoTopBarMenu)`).
    fn new_chat_menu(&self, dialog: &NewChatDialog, cx: &mut Context<Self>) -> AnyElement {
        let secs = self.new_chat_auto_delete(dialog);
        let value = (secs > 0).then(|| quill::auto_delete::format_ttl(secs));
        let owner = cx.entity().downgrade();
        Button::new("new-chat-menu")
            .icon(IconName::EllipsisVertical)
            .ghost()
            .small()
            .accessibility_label(tr(keys::PROFILE_ACTION_SHORT_MORE))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let value = value.clone();
                let owner = owner.clone();
                menu.item(
                    PopupMenuItem::element(move |_, cx| {
                        h_flex()
                            .w_full()
                            .font_normal()
                            .gap_4()
                            .justify_between()
                            .child(tr(keys::MANAGE_MESSAGES_TTL_MENU))
                            .children(value.clone().map(|value| {
                                div().text_color(cx.theme().muted_foreground).child(value)
                            }))
                    })
                    .icon(IconName::Timer)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.open_new_chat_auto_delete(window, cx);
                        });
                    }),
                )
            })
            .into_any_element()
    }

    pub(super) fn new_chat_footer(&self, view: &NewChatView, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.new_chat_dialog() else {
            return div().into_any_element();
        };
        let busy = view.busy;
        let ghost =
            |id: &'static str,
             label: Key,
             cx: &mut Context<Self>,
             action: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                Button::new(id)
                    .label(tr(label))
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        action(this, window, cx);
                        this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                    }))
            };
        let primary =
            |id: &'static str,
             label: Key,
             disabled: bool,
             cx: &mut Context<Self>,
             action: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                Button::new(id)
                    .label(tr(label))
                    .primary()
                    .loading(busy)
                    .disabled(busy || disabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        action(this, window, cx);
                        this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                    }))
            };
        let row = h_flex().w_full().justify_end().gap_2();
        let row = match view.step {
            NewChatStep::Info | NewChatStep::Done(_) => row
                .child(ghost("new-chat-cancel", keys::CANCEL, cx, |this, _, cx| {
                    this.close_new_chat(cx)
                }))
                .child(primary(
                    "new-chat-confirm",
                    dialog.kind.info_confirm(),
                    dialog.sources.is_open(),
                    cx,
                    |this, window, cx| this.confirm_new_chat_info(window, cx),
                )),
            NewChatStep::AutoDelete => {
                let disable = (self.new_chat_auto_delete(dialog) > 0).then(|| {
                    ghost(
                        "new-chat-ttl-disable",
                        keys::MANAGE_MESSAGES_TTL_DISABLE,
                        cx,
                        |this, _, cx| this.save_new_chat_auto_delete(true, cx),
                    )
                });
                row.children(disable)
                    .child(div().flex_1())
                    .child(ghost(
                        "new-chat-ttl-cancel",
                        keys::CANCEL,
                        cx,
                        |this, _, cx| {
                            this.set_new_chat_stage(quill::new_chat::NewChatStage::Info, cx)
                        },
                    ))
                    .child(primary(
                        "new-chat-ttl-save",
                        keys::SETTINGS_SAVE,
                        false,
                        cx,
                        |this, _, cx| this.save_new_chat_auto_delete(false, cx),
                    ))
            }
            NewChatStep::Members => row
                .child(ghost(
                    "new-chat-members-cancel",
                    keys::CANCEL,
                    cx,
                    |this, _, cx| this.set_new_chat_stage(quill::new_chat::NewChatStage::Info, cx),
                ))
                .child(primary(
                    "new-chat-create",
                    keys::CREATE_GROUP_CREATE,
                    false,
                    cx,
                    |this, _, cx| this.send_new_chat(quill::new_chat::NewChatBack::Members, cx),
                )),
            NewChatStep::Type(_) => row
                .child(ghost(
                    "new-chat-type-skip",
                    keys::CREATE_GROUP_SKIP,
                    cx,
                    |this, window, cx| this.dismiss_new_chat(window, cx),
                ))
                .child(primary(
                    "new-chat-type-save",
                    keys::SETTINGS_SAVE,
                    false,
                    cx,
                    |this, window, cx| this.save_group_type_box(window, cx),
                )),
            NewChatStep::Subscribers(_) => row
                .child(ghost(
                    "new-chat-add-skip",
                    keys::CREATE_GROUP_SKIP,
                    cx,
                    |this, window, cx| this.dismiss_new_chat(window, cx),
                ))
                .child(primary(
                    "new-chat-add",
                    keys::PARTICIPANT_INVITE,
                    dialog.selected_users.is_empty(),
                    cx,
                    |this, window, cx| this.add_new_chat_subscribers(window, cx),
                )),
        };
        row.into_any_element()
    }

    pub(super) fn new_chat_body(
        &mut self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = self.new_chat_view()?;
        self.new_chat_dialog()?;
        let failure = view
            .failure
            .as_ref()
            .and_then(|(failure, reason)| failure_text(*failure, reason))
            .map(|text| {
                div()
                    .id("new-chat-failure")
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(text)
            });
        let body = match view.step {
            NewChatStep::Info => {
                if let Some(panel) = self.photo_sources_panel(cx) {
                    return Some(panel);
                }
                let no_title = matches!(view.failure, Some((CreateFailure::NoTitle, _)));
                self.new_chat_info(no_title, cx)
            }
            NewChatStep::AutoDelete => {
                let dialog = self.new_chat_dialog()?;
                v_form()
                    .child(
                        field()
                            .label(tr(keys::MANAGE_MESSAGES_TTL_MENU))
                            .child(Select::new(&dialog.auto_delete_picker)),
                    )
                    .into_any_element()
            }
            NewChatStep::Members | NewChatStep::Subscribers(_) => self.new_chat_picker(cx),
            NewChatStep::Type(_) => self.group_type_box_body(cx)?,
            NewChatStep::Done(_) => return None,
        };
        Some(
            div()
                .id("new-chat-body")
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .children(failure)
                .into_any_element(),
        )
    }

    /// Photo, name and description (tdesktop `GroupInfoBox::prepare`).
    fn new_chat_info(&self, no_title: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.new_chat_dialog() else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (danger, primary, on_primary) = (theme.danger, theme.primary, theme.primary_foreground);
        // tdesktop's `UserpicButton::Role::ChoosePhoto` without a photo: a
        // camera on the accent color.
        let fallback = div()
            .size(px(PHOTO_PX))
            .rounded_full()
            .bg(primary)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Camera)
                    .size(px(30.))
                    .text_color(on_primary),
            )
            .into_any_element();
        let photo = self.chat_photo_button(
            "new-chat-photo",
            dialog.photo.as_ref(),
            fallback,
            PhotoTarget::NewChat,
            dialog.photo_note.clone(),
            dialog.photo.is_some(),
            cx,
        );
        let name_label = tr(dialog.kind.name_label());
        let title_error = (dialog.title_error || no_title).then(|| {
            div()
                .text_xs()
                .text_color(danger)
                .child(tr(keys::Q_PEOPLE_TITLE_MUST_BE_1_128))
        });
        let name = v_form().child(
            field().label(name_label).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Input::new(&dialog.title_input).aria_label(name_label))
                    .children(title_error),
            ),
        );
        let description = dialog.kind.has_description().then(|| {
            let used = dialog.description_input.read(cx).value().chars().count();
            let left = DESCRIPTION_MAX as i64 - used as i64;
            let counter = (left < 0).then(|| {
                div()
                    .flex()
                    .justify_end()
                    .text_xs()
                    .text_color(danger)
                    .child(left.to_string())
            });
            v_form().child(
                field().label(tr(keys::CREATE_GROUP_DESCRIPTION)).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Textarea::new(&dialog.description_input)
                                .aria_label(tr(keys::CREATE_GROUP_DESCRIPTION)),
                        )
                        .children(counter),
                ),
            )
        });
        div()
            .id("new-chat-info")
            .flex()
            .flex_col()
            .gap_4()
            .child(
                h_flex()
                    .items_center()
                    .gap_4()
                    .child(photo)
                    .child(div().flex_1().min_w_0().child(name)),
            )
            .children(description)
            .into_any_element()
    }

    /// The members or subscribers picker (the contacts list the member
    /// dialog's Add uses).
    fn new_chat_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.new_chat_dialog() else {
            return div().into_any_element();
        };
        let query = dialog.search_input.read(cx).value().to_string();
        let selected = dialog.selected_users.clone();
        let search = Textarea::new(&dialog.search_input)
            .aria_label(tr(keys::Q_PEOPLE_SEARCH_PEOPLE_TO_INVITE))
            .h(px(40.));
        let rows = self.g1_contact_rows(&query, cx);
        let mut list = div().id("new-chat-contacts").flex().flex_col().gap_1();
        if rows.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::CONTACTS_NOT_FOUND)),
            );
        }
        for row in &rows {
            let picked = selected.contains(&row.user_id);
            list = list.child(self.g1_contact_checkbox(
                "g1-create".to_string(),
                row,
                picked,
                row.user_id,
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(search)
            .child(list)
            .into_any_element()
    }

    /// A private channel's link in the type step (tdesktop
    /// `SetupChannelBox`): "Invite link" and the link, which copies on a
    /// click; "Create an invite link" while it has none.
    pub(in crate::ui) fn new_chat_invite_link(
        &self,
        chat: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let link = self
            .session()
            .and_then(|s| s.chat_primary_invite_link(chat))
            .map(|link| link.invite_link.clone());
        let button = match link {
            Some(link) => Button::new("new-chat-invite-link")
                .icon(IconName::Copy)
                .label(link.clone())
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    this.connection
                        .set_note(StatusNote::info(tr(keys::CREATE_CHANNEL_LINK_COPIED)));
                    cx.notify();
                })),
            None => Button::new("new-chat-invite-link")
                .label(tr(keys::GROUP_INVITE_CREATE))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.replace_primary_chat_invite_link(chat);
                    }
                    cx.notify();
                })),
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::CREATE_GROUP_INVITE_LINK)),
            )
            .child(h_flex().child(button))
            .into_any_element()
    }
}

//! Methods moved out of `folder_share.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn build_folder_invite_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderInvite, |this, _, cx| {
                this.close_folder_invite(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.folders.invite.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Add folder"))
                    .on_close(on_close.clone());
            };
            let muted = cx.theme().muted_foreground;
            let session = this.session();
            let info = session.and_then(|s| s.chat_list.folder_invite_info.clone());
            let error = session.and_then(|s| s.chat_list.folder_invite_error.clone());
            let close_btn = |label: &'static str, cx: &mut Context<QuillApp>| {
                Button::new("folder-invite-close")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_folder_invite(cx);
                        this.close_kit_dialog_if_done(DialogKind::FolderInvite, window, cx);
                    }))
            };
            let mut body = div().flex().flex_col().gap_3();
            let Some(info) = info else {
                if let Some(error) = error.clone() {
                    body = body.child(
                        div()
                            .id("folder-invite-error")
                            .text_sm()
                            .text_color(danger_dark())
                            .child(format!(
                                "This folder link is invalid or has expired ({error})."
                            )),
                    );
                } else {
                    body = body.child(div().text_xs().text_color(muted).child("Checking link…"));
                }
                let footer = div()
                    .flex()
                    .justify_end()
                    .child(close_btn("Close", cx))
                    .into_any_element();
                return finish_dialog(
                    dialog,
                    "Add folder".into(),
                    body.into_any_element(),
                    Some(footer),
                    on_close,
                );
            };
            let folder_name = info.folder.name.clone();
            let exists = info.folder.id != 0;
            let nothing_new = info.missing_chat_ids.is_empty();
            let title = if exists && nothing_new {
                "Folder already added".to_string()
            } else if exists {
                "Add chats to folder".to_string()
            } else {
                "Add folder".to_string()
            };
            let intro = if exists && nothing_new {
                format!("You have already added the folder {folder_name} and all its chats.")
            } else if exists {
                format!("Do you want to join chats and add them to the folder {folder_name}?")
            } else {
                format!(
                    "Do you want to add the chat folder {folder_name} and join its groups and channels?"
                )
            };
            if let Some(error) = error.clone() {
                body = body.child(
                    div()
                        .id("folder-invite-error")
                        .text_sm()
                        .text_color(danger_dark())
                        .child(format!("Couldn’t add the folder: {error}")),
                );
            }
            body = body.child(div().text_sm().child(intro));
            let missing = info.missing_chat_ids.clone();
            if !nothing_new {
                let all_selected = missing.iter().all(|id| state.selected.contains(id));
                let select_ids = missing.clone();
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_xs().font_semibold().text_color(muted).child(
                            match missing.len() {
                                1 => "1 chat to join".to_string(),
                                n => format!("{n} chats to join"),
                            },
                        ))
                        .child(
                            Button::new("folder-invite-select-all")
                                .label(if all_selected {
                                    "Deselect all"
                                } else {
                                    "Select all"
                                })
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(dialog) = this.folders.invite.as_mut() {
                                        if select_ids.iter().all(|id| dialog.selected.contains(id)) {
                                            dialog.selected.clear();
                                        } else {
                                            dialog.selected = select_ids.iter().copied().collect();
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                );
                let mut list = div()
                    .id("folder-invite-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(240.))
                    .overflow_y_scroll();
                for chat_id in missing.iter().copied() {
                    let checked = state.selected.contains(&chat_id);
                    list = list.child(
                        Checkbox::new(("folder-invite-chat", chat_id as u64))
                            .checked(checked)
                            .label(chat_title(this.session(), chat_id))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(dialog) = this.folders.invite.as_mut() {
                                    if on {
                                        dialog.selected.insert(chat_id);
                                    } else {
                                        dialog.selected.remove(&chat_id);
                                    }
                                }
                                cx.notify();
                            })),
                    );
                }
                body = body
                    .child(list)
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("You can deselect the chats you don’t want to join."),
                    );
            }
            if !info.added_chat_ids.is_empty() {
                body = body.child(div().text_xs().text_color(muted).child(
                    match info.added_chat_ids.len() {
                        1 => "1 chat in this folder".to_string(),
                        n => format!("{n} chats in this folder"),
                    },
                ));
            }
            let selected = state.selected.len();
            let adding = state.adding;
            let action_label = if exists {
                match selected {
                    1 => "Join chat".to_string(),
                    _ => "Join chats".to_string(),
                }
            } else {
                format!("Add {folder_name}")
            };
            let can_confirm = !(exists && (nothing_new || selected == 0));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(close_btn(
                    if exists && nothing_new { "Close" } else { "Cancel" },
                    cx,
                ))
                .when(can_confirm, |footer| {
                    footer.child(
                        Button::new("folder-invite-add")
                            .label(action_label)
                            .loading(adding)
                            .disabled(adding)
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_folder_invite(cx))),
                    )
                })
                .into_any_element();
            finish_dialog(dialog, title, body.into_any_element(), Some(footer), on_close)
        })
    }
}

//! Shareable folders: the Share Folder dialog (invite links, tdesktop
//! `boxes/filters/edit_filter_links.cpp`) and the "Add folder" dialog for an
//! `addlist` link (tdesktop `ui/chatlist_box` / `boxes/filters`).

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::state::Session;
use quill::telegram::envelope::ChatFolderInviteLink;
use std::cell::RefCell;
use std::rc::Rc;

/// Link names are 0–32 characters (`createChatFolderInviteLink`).
const LINK_NAME_MAX: usize = 32;

/// Whether the folder's rules allow a link (tdesktop
/// `GoodForExportFilterLink`): only pinned and always-included chats may
/// make up a shared folder, so any type rule or excluded chat forbids it.
pub(crate) fn folder_can_be_shared(spec: &quill::telegram::envelope::ChatFolderSpec) -> bool {
    spec.excluded_chat_ids.is_empty()
        && !spec.include_contacts
        && !spec.include_non_contacts
        && !spec.include_bots
        && !spec.include_groups
        && !spec.include_channels
        && !spec.exclude_muted
        && !spec.exclude_read
        && !spec.exclude_archived
}

/// `t.me/addlist/x` without the scheme, for the row subtitle.
fn short_link(link: &str) -> &str {
    link.strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .unwrap_or(link)
}

fn chats_count_label(count: usize) -> String {
    match count {
        0 => "No chats selected".to_string(),
        1 => "1 chat selected".to_string(),
        n => format!("{n} chats selected"),
    }
}

/// Dialog chrome shared by both folder-link dialogs: title, scrollable
/// body, optional footer.
pub(super) fn finish_dialog(
    dialog: Dialog,
    title: String,
    body: AnyElement,
    footer: Option<AnyElement>,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Dialog {
    let body = Rc::new(RefCell::new(Some(body)));
    let dialog = dialog
        .overlay(true)
        .title(crate::ui::shell::dialog_title(title))
        .content(crate::ui::shell::scrollable_dialog_content(
            move |content, _, _| {
                let body = body
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element());
                content.child(body)
            },
        ));
    let dialog = match footer {
        Some(footer) => dialog.footer(footer),
        None => dialog,
    };
    dialog.on_close(on_close)
}

pub(super) fn chat_title(session: Option<&Session>, chat_id: i64) -> String {
    session
        .and_then(|s| s.chats.get(&chat_id))
        .map(|c| c.title.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Chat".to_string())
}

impl QuillApp {
    fn session_mut_any(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    // ------------------------------------------------------------------
    // Share Folder
    // ------------------------------------------------------------------

    pub(super) fn open_folder_share(
        &mut self,
        folder_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.folders.share = Some(FolderShareDialog::new(window, cx, folder_id));
        if let Some(live) = self.live.as_mut() {
            if !live.driver.session.folder_specs.contains_key(&folder_id)
                && let Err(err) = live.driver.fetch_chat_folder(folder_id)
            {
                self.connection.status_note = format!("could not load folder: {err:?}");
            }
            if let Err(err) = live.driver.fetch_folder_share(folder_id) {
                self.connection.status_note = format!("could not load folder links: {err:?}");
            }
        }
        cx.notify();
    }

    pub(super) fn close_folder_share(&mut self, cx: &mut Context<Self>) {
        self.folders.share = None;
        cx.notify();
    }

    /// Chats a link may include, in the list's order.
    fn folder_link_choices(&self, folder_id: i32) -> Vec<i64> {
        self.session()
            .and_then(|s| s.folder_link_chats.get(&folder_id).cloned())
            .unwrap_or_default()
    }

    fn start_folder_link_form(
        &mut self,
        link: Option<ChatFolderInviteLink>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(folder_id) = self.folders.share.as_ref().map(|d| d.folder_id) else {
            return;
        };
        // tdesktop `FilterLinksLimitBox`: a folder has a few links at most.
        if link.is_none() {
            let full = self.session().is_some_and(|s| {
                let count = s.folder_invite_links.get(&folder_id).map_or(0, Vec::len);
                s.folder_limits.links_full(count, s.my_is_premium())
            });
            if full {
                self.show_folder_limit(quill::folder_limits::FolderLimitKind::InviteLinks, cx);
                return;
            }
        }
        let choices = self.folder_link_choices(folder_id);
        let name = link.as_ref().map(|l| l.name.clone()).unwrap_or_default();
        let selected: std::collections::HashSet<i64> = match &link {
            Some(link) => link.chat_ids.iter().copied().collect(),
            // tdesktop starts a new link with every shareable chat ticked.
            None => choices.iter().copied().collect(),
        };
        if let Some(dialog) = self.folders.share.as_mut() {
            dialog.view = FolderShareView::Edit {
                link: link.map(|l| l.invite_link),
            };
            dialog.selected = selected;
            dialog.error = None;
            dialog.confirm_delete = None;
            dialog
                .name_input
                .update(cx, |input, cx| input.set_value(name, window, cx));
        }
        cx.notify();
    }

    fn save_folder_link(&mut self, cx: &mut Context<Self>) {
        let Some((folder_id, view, mut chat_ids, name)) = self.folders.share.as_ref().map(|d| {
            let mut ids: Vec<i64> = d.selected.iter().copied().collect();
            ids.sort_unstable();
            (
                d.folder_id,
                d.view.clone(),
                ids,
                d.name_input.read(cx).value().trim().to_string(),
            )
        }) else {
            return;
        };
        let fail = |this: &mut Self, text: &str| {
            if let Some(dialog) = this.folders.share.as_mut() {
                dialog.error = Some(text.to_string());
            }
        };
        if name.chars().count() > LINK_NAME_MAX {
            fail(self, "Link names are at most 32 characters.");
            cx.notify();
            return;
        }
        if chat_ids.is_empty() {
            fail(self, "Please choose at least one chat for this folder.");
            cx.notify();
            return;
        }
        let FolderShareView::Edit { link } = view else {
            return;
        };
        chat_ids.dedup();
        let result = match self.live.as_mut() {
            Some(live) => match &link {
                Some(link) => live
                    .driver
                    .edit_folder_invite_link(folder_id, link, &name, &chat_ids)
                    .map(|_| ()),
                None => live
                    .driver
                    .create_folder_invite_link(folder_id, &name, &chat_ids)
                    .map(|_| ()),
            },
            None => {
                // Screenshot demo: apply locally.
                self.apply_demo_folder_link(folder_id, link.clone(), name.clone(), chat_ids);
                if let Some(dialog) = self.folders.share.as_mut() {
                    dialog.view = FolderShareView::List;
                }
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                if self.live.is_some()
                    && let Some(dialog) = self.folders.share.as_mut()
                {
                    dialog.busy = true;
                    dialog.error = None;
                }
            }
            Err(err) => fail(self, &format!("Couldn't save the link: {err:?}")),
        }
        cx.notify();
    }

    fn apply_demo_folder_link(
        &mut self,
        folder_id: i32,
        link: Option<String>,
        name: String,
        chat_ids: Vec<i64>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let links = session.folder_invite_links.entry(folder_id).or_default();
        match link {
            Some(url) => {
                if let Some(existing) = links.iter_mut().find(|l| l.invite_link == url) {
                    existing.name = name;
                    existing.chat_ids = chat_ids;
                }
            }
            None => links.push(ChatFolderInviteLink {
                invite_link: format!("https://t.me/addlist/demo{}", links.len() + 1),
                name,
                chat_ids,
            }),
        }
        if let Some(info) = session.chat_folders.iter_mut().find(|f| f.id == folder_id) {
            info.is_shareable = true;
            info.has_my_invite_links = true;
        }
    }

    fn delete_folder_link(&mut self, link: &str, cx: &mut Context<Self>) {
        let Some(folder_id) = self.folders.share.as_ref().map(|d| d.folder_id) else {
            return;
        };
        let result = match self.live.as_mut() {
            Some(live) => live
                .driver
                .delete_folder_invite_link(folder_id, link)
                .map(|_| ()),
            None => {
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(links) = session.folder_invite_links.get_mut(&folder_id)
                {
                    links.retain(|l| l.invite_link != link);
                }
                Ok(())
            }
        };
        if let Some(dialog) = self.folders.share.as_mut() {
            dialog.confirm_delete = None;
            if let Err(err) = result {
                dialog.error = Some(format!("Couldn't delete the link: {err:?}"));
            }
        }
        cx.notify();
    }

    fn copy_folder_link(&mut self, link: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(link.to_string()));
        if let Some(dialog) = self.folders.share.as_mut() {
            dialog.copied = Some(link.to_string());
        }
        self.connection.status_note = "Link copied to clipboard".into();
        cx.notify();
    }

    /// Poll-loop half: drain the one-shot results of link requests into the
    /// open dialog. Returns whether anything changed.
    pub(super) fn drain_folder_share(&mut self, cx: &mut Context<Self>) -> bool {
        let (saved, error) = match self.session_mut_any() {
            Some(session) => (
                std::mem::take(&mut session.folder_link_saved),
                session.folder_share_error.take(),
            ),
            None => return false,
        };
        let mut changed = false;
        if let Some(dialog) = self.folders.share.as_mut() {
            if saved {
                dialog.view = FolderShareView::List;
                dialog.busy = false;
                dialog.error = None;
                changed = true;
            }
            if let Some(error) = error {
                dialog.busy = false;
                dialog.error = Some(error);
                changed = true;
            }
        } else if let Some(error) = error {
            // Recommended folders (or a stray failure) report in the toast.
            self.connection.status_note = error;
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn build_folder_share_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderShare, |this, _, cx| {
                this.close_folder_share(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.folders.share.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Share folder"))
                    .on_close(on_close.clone());
            };
            let folder_id = state.folder_id;
            let name = this
                .session()
                .and_then(|s| s.chat_folders.iter().find(|f| f.id == folder_id))
                .map(|f| f.name.clone())
                .unwrap_or_default();
            let editing = matches!(state.view, FolderShareView::Edit { .. });
            let title = match &state.view {
                FolderShareView::List => "Share folder".to_string(),
                FolderShareView::Edit { link: None } => "New invite link".to_string(),
                FolderShareView::Edit { link: Some(_) } => "Edit invite link".to_string(),
            };
            let (body, footer) = if editing {
                (this.folder_link_form(cx), this.folder_link_form_footer(cx))
            } else {
                (
                    this.folder_link_list(&name, cx),
                    div()
                        .flex()
                        .justify_end()
                        .child(Button::new("folder-share-done").label("Done").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.close_folder_share(cx);
                                this.close_kit_dialog_if_done(DialogKind::FolderShare, window, cx);
                            }),
                        ))
                        .into_any_element(),
                )
            };
            finish_dialog(dialog, title, body, Some(footer), on_close)
        })
    }

    fn folder_link_list(&self, folder_name: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.folders.share.as_ref() else {
            return div().into_any_element();
        };
        let folder_id = state.folder_id;
        let muted = cx.theme().muted_foreground;
        let spec = self
            .session()
            .and_then(|s| s.folder_specs.get(&folder_id).cloned());
        let links = self
            .session()
            .and_then(|s| s.folder_invite_links.get(&folder_id).cloned());
        let mut body = div().flex().flex_col().gap_3();
        if let Some(error) = state.error.clone() {
            body = body.child(
                div()
                    .id("folder-share-error")
                    .text_xs()
                    .text_color(danger_dark())
                    .child(error),
            );
        }
        let Some(spec) = spec else {
            return body
                .child(div().text_xs().text_color(muted).child("Loading folder…"))
                .into_any_element();
        };
        if !folder_can_be_shared(&spec) {
            return body
                .child(div().text_sm().child(
                    "You can’t share folders which include or exclude specific chat \
                     types like ‘Groups’, ‘Contacts’, etc.",
                ))
                .into_any_element();
        }
        body = body.child(div().text_xs().text_color(muted).child(format!(
            "Share access to some of this folder’s groups and channels with others. \
             Anyone with a link can add the {folder_name} folder and the chats selected."
        )));
        let Some(links) = links else {
            return body
                .child(div().text_xs().text_color(muted).child("Loading links…"))
                .into_any_element();
        };
        let mut list = div().id("folder-share-links").flex().flex_col().gap_1();
        for link in links {
            let url = link.invite_link.clone();
            let label = if link.name.is_empty() {
                "Invite link".to_string()
            } else {
                link.name.clone()
            };
            let subtitle = format!(
                "{} · {}",
                short_link(&link.invite_link),
                match link.chat_ids.len() {
                    1 => "1 chat".to_string(),
                    n => format!("{n} chats"),
                }
            );
            let confirming = state.confirm_delete.as_deref() == Some(url.as_str());
            let copied = state.copied.as_deref() == Some(url.as_str());
            let copy_url = url.clone();
            let edit_link = link.clone();
            let ask_url = url.clone();
            let delete_url = url.clone();
            let mut row = div()
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().font_medium().truncate().child(label))
                                .child(
                                    div().text_xs().text_color(muted).truncate().child(subtitle),
                                ),
                        )
                        .child(
                            Button::new(format!("folder-link-copy-{url}"))
                                .icon(if copied {
                                    gpui_kit::assets::IconName::Check
                                } else {
                                    gpui_kit::assets::IconName::Copy
                                })
                                .ghost()
                                .small()
                                .tooltip("Copy link")
                                .accessibility_label("Copy invite link")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.copy_folder_link(&copy_url, cx);
                                })),
                        )
                        .child(
                            Button::new(format!("folder-link-edit-{url}"))
                                .icon(gpui_kit::assets::IconName::Pencil)
                                .ghost()
                                .small()
                                .tooltip("Edit link")
                                .accessibility_label("Edit invite link")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_folder_link_form(
                                        Some(edit_link.clone()),
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(format!("folder-link-delete-{url}"))
                                .icon(gpui_kit::assets::IconName::Trash)
                                .ghost()
                                .small()
                                .tooltip("Delete link")
                                .accessibility_label("Delete invite link")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(dialog) = this.folders.share.as_mut() {
                                        dialog.confirm_delete = Some(ask_url.clone());
                                    }
                                    cx.notify();
                                })),
                        ),
                );
            if confirming {
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .child("Are you sure you want to delete this link?"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(
                                    Button::new(format!("folder-link-delete-no-{url}"))
                                        .label("Cancel")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(dialog) = this.folders.share.as_mut() {
                                                dialog.confirm_delete = None;
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(format!("folder-link-delete-yes-{url}"))
                                        .label("Delete")
                                        .small()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.delete_folder_link(&delete_url, cx);
                                        })),
                                ),
                        ),
                );
            }
            list = list.child(row);
        }
        body = body.child(list);
        let choices = self.folder_link_choices(folder_id);
        let loaded = self
            .session()
            .is_some_and(|s| s.folder_link_chats.contains_key(&folder_id));
        if loaded && choices.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("There are no chats in this folder that you can share with others."),
            );
        } else {
            body = body.child(
                Button::new("folder-link-create")
                    .icon(gpui_kit::assets::IconName::Link)
                    .label("Create an invite link")
                    .disabled(!loaded)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start_folder_link_form(None, window, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    fn folder_link_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.folders.share.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let choices = self.folder_link_choices(state.folder_id);
        let session = self.session();
        let all_selected =
            !choices.is_empty() && choices.iter().all(|id| state.selected.contains(id));
        let mut body = div().flex().flex_col().gap_2();
        if let Some(error) = state.error.clone() {
            body = body.child(
                div()
                    .id("folder-link-form-error")
                    .text_xs()
                    .text_color(danger_dark())
                    .child(error),
            );
        }
        body = body
            .child(Textarea::new(&state.name_input).aria_label("Invite link name"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(chats_count_label(state.selected.len())),
                    )
                    .child(
                        Button::new("folder-link-select-all")
                            .label(if all_selected {
                                "Deselect all"
                            } else {
                                "Select all"
                            })
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let choices = this
                                    .folders
                                    .share
                                    .as_ref()
                                    .map(|d| this.folder_link_choices(d.folder_id))
                                    .unwrap_or_default();
                                if let Some(dialog) = this.folders.share.as_mut() {
                                    if choices.iter().all(|id| dialog.selected.contains(id)) {
                                        dialog.selected.clear();
                                    } else {
                                        dialog.selected = choices.into_iter().collect();
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(div().text_xs().text_color(muted).child(
                "Select groups and channels that you want everyone who adds the folder \
                 via invite link to join.",
            ));
        let mut list = div()
            .id("folder-link-chats")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(260.))
            .overflow_y_scroll();
        for chat_id in choices {
            let checked = state.selected.contains(&chat_id);
            list = list.child(
                Checkbox::new(("folder-link-chat", chat_id as u64))
                    .checked(checked)
                    .label(chat_title(session, chat_id))
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(dialog) = this.folders.share.as_mut() {
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
        body.child(list).into_any_element()
    }

    fn folder_link_form_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.folders.share.as_ref().is_some_and(|d| d.busy);
        let creating = matches!(
            self.folders.share.as_ref().map(|d| &d.view),
            Some(FolderShareView::Edit { link: None })
        );
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("folder-link-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(dialog) = this.folders.share.as_mut() {
                            dialog.view = FolderShareView::List;
                            dialog.error = None;
                            dialog.busy = false;
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("folder-link-save")
                    .label(if creating { "Create" } else { "Save" })
                    .loading(busy)
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.save_folder_link(cx))),
            )
            .into_any_element()
    }

    // ------------------------------------------------------------------
    // Add folder by link
    // ------------------------------------------------------------------

    /// `addlist` link opened: show the dialog and check the link.
    pub(super) fn open_folder_invite(&mut self, link: String, cx: &mut Context<Self>) {
        self.folders.invite = Some(FolderInviteDialog::new(link.clone()));
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.check_folder_invite_link(&link)
        {
            self.connection.status_note = format!("could not check the folder link: {err:?}");
        }
        cx.notify();
    }

    pub(super) fn close_folder_invite(&mut self, cx: &mut Context<Self>) {
        self.folders.invite = None;
        if let Some(session) = self.session_mut_any() {
            session.folder_invite_link = None;
            session.folder_invite_info = None;
            session.folder_invite_error = None;
            session.folder_invite_done = false;
        }
        cx.notify();
    }

    /// Poll-loop half of the add-by-link dialog: seed the ticked chats once
    /// the link is checked, fetch the titles of chats we do not know yet,
    /// and finish when the add is confirmed.
    pub(super) fn drive_folder_invite(&mut self, cx: &mut Context<Self>) -> bool {
        if self.folders.invite.is_none() {
            return false;
        }
        let (info, done) = match self.session() {
            Some(s) => (s.folder_invite_info.clone(), s.folder_invite_done),
            None => return false,
        };
        let mut changed = false;
        if done {
            let name = info.as_ref().map(|i| i.folder.name.clone());
            let joined = self
                .folders
                .invite
                .as_ref()
                .map(|d| d.selected.len())
                .unwrap_or(0);
            self.close_folder_invite(cx);
            self.connection.status_note = match (name, joined) {
                (Some(name), 0) => format!("Folder {name} added"),
                (Some(name), n) => format!("Folder {name} added · you also joined {n} chat(s)"),
                (None, _) => "Folder added".into(),
            };
            return true;
        }
        if let Some(info) = info {
            let unknown: Vec<i64> = info
                .missing_chat_ids
                .iter()
                .chain(info.added_chat_ids.iter())
                .copied()
                .filter(|id| self.session().is_some_and(|s| !s.chats.contains_key(id)))
                .collect();
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.fetch_chats_for_folder_invite(&unknown);
            }
            if let Some(dialog) = self.folders.invite.as_mut()
                && !dialog.seeded
            {
                dialog.seeded = true;
                dialog.selected = info.missing_chat_ids.iter().copied().collect();
                changed = true;
            }
        }
        if let Some(dialog) = self.folders.invite.as_mut()
            && dialog.adding
            && self
                .live
                .as_ref()
                .is_some_and(|l| l.driver.session.folder_invite_error.is_some())
        {
            dialog.adding = false;
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    fn confirm_folder_invite(&mut self, cx: &mut Context<Self>) {
        let Some((link, selected)) = self.folders.invite.as_ref().map(|d| {
            let mut ids: Vec<i64> = d.selected.iter().copied().collect();
            ids.sort_unstable();
            (d.link.clone(), ids)
        }) else {
            return;
        };
        let info = self.session().and_then(|s| s.folder_invite_info.clone());
        let Some(info) = info else { return };
        // A new folder keeps every chat of the link; chats already joined
        // are part of it whatever was ticked.
        let mut chat_ids = selected;
        if info.folder.id == 0 {
            chat_ids.extend(info.added_chat_ids.iter().copied());
            chat_ids.sort_unstable();
            chat_ids.dedup();
        }
        match self.live.as_mut() {
            Some(live) => match live.driver.add_folder_by_invite_link(&link, &chat_ids) {
                Ok(_) => {
                    if let Some(dialog) = self.folders.invite.as_mut() {
                        dialog.adding = true;
                    }
                }
                Err(err) => {
                    if let Some(session) = self.session_mut_any() {
                        session.folder_invite_error =
                            Some(format!("Couldn't add the folder: {err:?}"));
                    }
                }
            },
            None => {
                // Screenshot demo: finish without a server.
                if let Some(session) = self.demo_session.as_mut() {
                    session.folder_invite_done = true;
                }
            }
        }
        cx.notify();
    }

    pub(super) fn build_folder_invite_dialog(
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
            let info = session.and_then(|s| s.folder_invite_info.clone());
            let error = session.and_then(|s| s.folder_invite_error.clone());
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

crate::ui::shell::register_dialogs! {
    FolderShare => DialogSpec::new(
        1900,
        |app| app.folders.share.is_some(),
        QuillApp::build_folder_share_dialog,
    ),

    FolderInvite => DialogSpec::new(
        2000,
        |app| app.folders.invite.is_some(),
        QuillApp::build_folder_invite_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{chats_count_label, folder_can_be_shared, short_link};
    use quill::telegram::envelope::ChatFolderSpec;

    #[test]
    fn only_plain_folders_can_be_shared() {
        let plain = ChatFolderSpec {
            included_chat_ids: vec![1, 2],
            pinned_chat_ids: vec![3],
            ..ChatFolderSpec::default()
        };
        assert!(folder_can_be_shared(&plain));
        for tweak in [
            |s: &mut ChatFolderSpec| s.include_groups = true,
            |s: &mut ChatFolderSpec| s.include_contacts = true,
            |s: &mut ChatFolderSpec| s.exclude_muted = true,
            |s: &mut ChatFolderSpec| s.exclude_read = true,
            |s: &mut ChatFolderSpec| s.excluded_chat_ids.push(9),
        ] {
            let mut spec = plain.clone();
            tweak(&mut spec);
            assert!(!folder_can_be_shared(&spec));
        }
    }

    #[test]
    fn links_shorten_for_display() {
        assert_eq!(short_link("https://t.me/addlist/abc"), "t.me/addlist/abc");
        assert_eq!(short_link("tg://addlist?slug=x"), "tg://addlist?slug=x");
        assert_eq!(chats_count_label(0), "No chats selected");
        assert_eq!(chats_count_label(1), "1 chat selected");
        assert_eq!(chats_count_label(4), "4 chats selected");
    }
}

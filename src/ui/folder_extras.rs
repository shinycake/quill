//! Folder follow-ups: the right-click menu of a folder tab, the shared
//! folder's "N new chats" bar and its join dialog, the Premium limit boxes
//! and the tag colour picker (tdesktop `window/window_filters_menu.cpp`,
//! `ui/chat/more_chats_bar.cpp`, `boxes/premium_limits_box.cpp`,
//! `boxes/filters/edit_filter_box.cpp`).

use super::app::QuillApp;
use super::chat_theme::{accent, bg_canvas, peer_name_color, text_menu};
use super::folder_share::{chat_title, finish_dialog};
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::folder_limits::{
    FolderLimitKind, TAG_COLOR_COUNT, limit_box_text, new_chats_bar_text, tag_choice_allowed,
    tag_picker_visible, tag_shown_choice,
};
use std::collections::HashSet;

/// Where Premium is bought (the official bot, as in tdesktop's fallback).
const PREMIUM_URL: &str = "https://t.me/PremiumBot";

/// The right-click menu of a folder tab: which folder (`None` = the All
/// tab) and where the click landed.
#[derive(Clone, Copy, Debug)]
pub(super) struct FolderTabMenu {
    pub folder_id: Option<i32>,
    pub position: Point<Pixels>,
}

/// The join dialog of a shared folder's new chats.
pub(super) struct FolderNewChatsDialog {
    pub folder_id: i32,
    pub selected: HashSet<i64>,
}

/// A tag chip: the folder name in capitals, tinted with the folder colour
/// (tdesktop `Ui::ChatsFilterTag`).
pub(super) fn tag_chip(text: String, color_id: i32) -> Div {
    let color = peer_name_color(color_id);
    div()
        .flex_none()
        .px(px(5.))
        .rounded_sm()
        .bg(color.opacity(0.16))
        .text_color(color)
        .text_size(px(10.))
        .line_height(px(15.))
        .font_semibold()
        .child(text)
}

impl QuillApp {
    fn is_premium(&self) -> bool {
        self.session().is_some_and(|s| s.my_is_premium())
    }

    // ------------------------------------------------------------------
    // Folder tab menu
    // ------------------------------------------------------------------

    pub(super) fn open_folder_tab_menu(
        &mut self,
        folder_id: Option<i32>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.message_ui.menu = None;
        self.chat_list.menu = None;
        self.chat_list.archive_menu = None;
        self.folders.tab_menu = Some(FolderTabMenu {
            folder_id,
            position,
        });
        cx.notify();
    }

    /// Whether any chat of the folder (or any chat at all for `None`) is
    /// unread: the menu shows "Mark as read" only then (tdesktop
    /// `MarkAsReadMenu::AddChatListAction`).
    fn folder_has_unread(&self, folder_id: Option<i32>) -> bool {
        self.session().is_some_and(|s| {
            s.chats.values().any(|c| {
                c.is_unread()
                    && match folder_id {
                        Some(id) => c.folder_positions.contains_key(&id),
                        None => c.in_main_list,
                    }
            })
        })
    }

    /// "Mark as read" for one folder (`readChatList`), or the main list.
    pub(super) fn mark_folder_read(&mut self, folder_id: Option<i32>, cx: &mut Context<Self>) {
        let Some(folder_id) = folder_id else {
            self.mark_all_chats_as_read(false, cx);
            return;
        };
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.mark_folder_as_read(folder_id) {
                self.connection.status_note = format!("mark as read failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            for chat in session.chats.values_mut() {
                if chat.folder_positions.contains_key(&folder_id) {
                    chat.unread_count = 0;
                    chat.is_marked_as_unread = false;
                }
            }
        }
        cx.notify();
    }

    /// The menu itself, in tdesktop's order: Edit folder, Mark as read,
    /// Remove (plus Share folder, which Quill keeps beside Edit). The All
    /// tab offers Mark all as read and the folder settings.
    pub(super) fn folder_tab_menu_overlay(
        &self,
        menu: FolderTabMenu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = cx.theme().accent;
        let item = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    attention: bool,
                    on_click: Box<dyn Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>,
                    cx: &mut Context<QuillApp>| {
            let color = if attention {
                Hsla::from(danger_dark())
            } else {
                Hsla::from(text_menu())
            };
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .text_color(color)
                .hover(|style| style.bg(hover))
                .role(Role::MenuItem)
                .aria_label(label)
                .child(Icon::new(icon).size(px(16.)).text_color(color))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.folders.tab_menu = None;
                    on_click(this, window, cx);
                    cx.notify();
                }))
                .into_any_element()
        };
        let unread = self.folder_has_unread(menu.folder_id);
        let mut rows: Vec<AnyElement> = Vec::new();
        match menu.folder_id {
            Some(folder_id) => {
                rows.push(item(
                    "folder-menu-edit",
                    IconName::Pencil,
                    "Edit folder",
                    false,
                    Box::new(move |this, window, cx| this.open_folder_edit(folder_id, window, cx)),
                    cx,
                ));
                rows.push(item(
                    "folder-menu-share",
                    IconName::Link,
                    "Share folder",
                    false,
                    Box::new(move |this, window, cx| this.open_folder_share(folder_id, window, cx)),
                    cx,
                ));
                if unread {
                    rows.push(item(
                        "folder-menu-read",
                        IconName::CircleCheck,
                        "Mark as read",
                        false,
                        Box::new(move |this, _, cx| this.mark_folder_read(Some(folder_id), cx)),
                        cx,
                    ));
                }
                rows.push(item(
                    "folder-menu-remove",
                    IconName::Trash,
                    "Remove",
                    true,
                    Box::new(move |this, _, cx| this.open_folder_delete(folder_id, cx)),
                    cx,
                ));
            }
            None => {
                if unread {
                    rows.push(item(
                        "folder-menu-read-all",
                        IconName::CircleCheck,
                        "Mark all as read",
                        false,
                        Box::new(|this, _, cx| this.mark_folder_read(None, cx)),
                        cx,
                    ));
                }
                rows.push(item(
                    "folder-menu-setup",
                    IconName::Settings,
                    "Edit folders",
                    false,
                    Box::new(|this, _, cx| this.open_folder_manage(cx)),
                    cx,
                ));
            }
        }
        let panel = div()
            .id("folder-tab-menu-panel")
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows);
        div()
            .id("folder-tab-menu-overlay")
            .track_focus(&self.frame.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("folder-tab-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.folders.tab_menu = None;
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            this.folders.tab_menu = None;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("folder-tab-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }

    // ------------------------------------------------------------------
    // "N new chats" bar of a shared folder
    // ------------------------------------------------------------------

    /// Ask for the open shared folder's new chats (the driver keeps to the
    /// update period). Called when a folder opens and from the poll loop.
    pub(super) fn poll_folder_new_chats(&mut self) {
        let (Some(folder_id), Some(live)) = (self.folders.tab, self.live.as_mut()) else {
            return;
        };
        let _ = live.driver.fetch_folder_new_chats(folder_id);
    }

    /// The bar above the list of a shared folder with new chats.
    pub(super) fn folder_new_chats_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let folder_id = self.folders.tab?;
        let count = self
            .session()?
            .folder_new_chats
            .get(&folder_id)
            .map_or(0, Vec::len);
        if count == 0 {
            return None;
        }
        let (title, subtitle) = new_chats_bar_text(count);
        Some(
            div()
                .id("folder-new-chats-bar")
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                .gap_2()
                .pl_3()
                .pr_1()
                .py_1()
                .bg(cx.theme().accent.opacity(0.10))
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("folder-new-chats-open")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label(format!("{title}. {subtitle}"))
                        .tab_index(0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_folder_new_chats(folder_id, cx);
                        }))
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .truncate()
                                .text_color(Hsla::from(accent()))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(cx.theme().muted_foreground)
                                .child(subtitle),
                        ),
                )
                .child(
                    Button::new("folder-new-chats-dismiss")
                        .icon(IconName::X)
                        .small()
                        .ghost()
                        .tooltip("Hide")
                        .accessibility_label("Hide new chats")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dismiss_folder_new_chats(folder_id, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// The bar's ✕: tell Telegram the offer is hidden (empty list).
    fn dismiss_folder_new_chats(&mut self, folder_id: i32, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.process_folder_new_chats(folder_id, &[]) {
                    self.connection.status_note = format!("could not hide new chats: {err:?}");
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.folder_new_chats.remove(&folder_id);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn open_folder_new_chats(&mut self, folder_id: i32, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self
            .session()
            .and_then(|s| s.folder_new_chats.get(&folder_id).cloned())
            .unwrap_or_default();
        self.folders.new_chats_dialog = Some(FolderNewChatsDialog {
            folder_id,
            selected: ids.into_iter().collect(),
        });
        cx.notify();
    }

    pub(super) fn close_folder_new_chats(&mut self, cx: &mut Context<Self>) {
        self.folders.new_chats_dialog = None;
        cx.notify();
    }

    fn confirm_folder_new_chats(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.folders.new_chats_dialog.take() else {
            return;
        };
        let mut ids: Vec<i64> = dialog.selected.iter().copied().collect();
        ids.sort_unstable();
        match self.live.as_mut() {
            Some(live) => match live.driver.process_folder_new_chats(dialog.folder_id, &ids) {
                Ok(_) => {
                    self.connection.status_note = match ids.len() {
                        0 => "new chats hidden".into(),
                        1 => "joining 1 chat…".into(),
                        n => format!("joining {n} chats…"),
                    }
                }
                Err(err) => {
                    self.connection.status_note = format!("could not join the chats: {err:?}")
                }
            },
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.folder_new_chats.remove(&dialog.folder_id);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn build_folder_new_chats_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderNewChats, |this, _, cx| {
                this.close_folder_new_chats(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.folders.new_chats_dialog.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Add chats to folder"))
                    .on_close(on_close.clone());
            };
            let muted = cx.theme().muted_foreground;
            let folder_id = state.folder_id;
            let folder_name = this
                .session()
                .and_then(|s| s.chat_folders.iter().find(|f| f.id == folder_id))
                .map(|f| f.name.clone())
                .unwrap_or_default();
            let chats: Vec<i64> = this
                .session()
                .and_then(|s| s.folder_new_chats.get(&folder_id).cloned())
                .unwrap_or_default();
            let all_selected =
                !chats.is_empty() && chats.iter().all(|id| state.selected.contains(id));
            let select_ids = chats.clone();
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(format!(
                    "Do you want to join chats and add them to the folder {folder_name}?"
                )));
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xs().font_semibold().text_color(muted).child(
                        match chats.len() {
                            1 => "1 chat to join".to_string(),
                            n => format!("{n} chats to join"),
                        },
                    ))
                    .child(
                        Button::new("folder-new-chats-select-all")
                            .label(if all_selected {
                                "Deselect all"
                            } else {
                                "Select all"
                            })
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(dialog) = this.folders.new_chats_dialog.as_mut() {
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
                .id("folder-new-chats-list")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(240.))
                .overflow_y_scroll();
            for chat_id in chats {
                let checked = state.selected.contains(&chat_id);
                list = list.child(
                    Checkbox::new(("folder-new-chat", chat_id as u64))
                        .checked(checked)
                        .label(chat_title(this.session(), chat_id))
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(dialog) = this.folders.new_chats_dialog.as_mut() {
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
            body = body.child(list).child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("You can deselect the chats you don’t want to join."),
            );
            let selected = state.selected.len();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("folder-new-chats-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_folder_new_chats(cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderNewChats, window, cx);
                        })),
                )
                .child(
                    Button::new("folder-new-chats-join")
                        .label(if selected == 1 {
                            "Join chat"
                        } else {
                            "Join chats"
                        })
                        .disabled(selected == 0)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_folder_new_chats(cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderNewChats, window, cx);
                        })),
                )
                .into_any_element();
            finish_dialog(
                dialog,
                "Add chats to folder".into(),
                body.into_any_element(),
                Some(footer),
                on_close,
            )
        })
    }

    // ------------------------------------------------------------------
    // Limit boxes
    // ------------------------------------------------------------------

    /// Open the limit box for `kind` and ask Telegram for its Premium
    /// value when we do not know it yet.
    pub(super) fn show_folder_limit(&mut self, kind: FolderLimitKind, cx: &mut Context<Self>) {
        self.folders.limit_box = Some(kind);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_premium_limit(kind);
        }
        cx.notify();
    }

    /// Poll-loop half: a request that failed on a limit opens its box and
    /// frees the dialog that was waiting for the answer.
    pub(super) fn drain_folder_limit(&mut self, cx: &mut Context<Self>) -> bool {
        let hit = match self.live.as_mut() {
            Some(live) => live.driver.session.folder_limit_hit.take(),
            None => self
                .demo_session
                .as_mut()
                .and_then(|s| s.folder_limit_hit.take()),
        };
        let Some(kind) = hit else {
            return false;
        };
        if let Some(dialog) = self.folders.share.as_mut() {
            dialog.busy = false;
        }
        if let Some(dialog) = self.folders.invite.as_mut() {
            dialog.adding = false;
        }
        self.show_folder_limit(kind, cx);
        true
    }

    pub(super) fn build_folder_limit_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderLimit, |this, _, cx| {
                this.folders.limit_box = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(kind) = this.folders.limit_box else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Limit Reached"))
                    .on_close(on_close.clone());
            };
            let premium = this.is_premium();
            let (current, premium_value) = this
                .session()
                .map(|s| {
                    (
                        s.folder_limits.current(kind, premium),
                        s.folder_limits.premium_value(kind),
                    )
                })
                .unwrap_or((0, 0));
            let (title, message) = limit_box_text(kind, current, premium_value, premium);
            let muted = cx.theme().muted_foreground;
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().id("folder-limit-text").text_sm().child(message));
            // Free versus Premium, as the bar of tdesktop's limit box.
            if kind != FolderLimitKind::Tags {
                let pill = |label: &'static str, value: i32, active: bool, cx: &App| {
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(if active {
                            cx.theme().accent.opacity(0.18)
                        } else {
                            cx.theme().muted
                        })
                        .text_sm()
                        .child(div().text_color(muted).child(label))
                        .child(div().font_semibold().child(value.to_string()))
                };
                let free = this
                    .session()
                    .map_or(current, |s| s.folder_limits.default_value(kind));
                let plus = premium_value;
                body = body.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(pill("Free", free, !premium, cx))
                        .child(pill("Premium", plus, premium, cx)),
                );
            }
            let mut footer = div().flex().justify_end().gap_2().child(
                Button::new("folder-limit-close")
                    .label(if premium { "OK" } else { "Not now" })
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.folders.limit_box = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::FolderLimit, window, cx);
                    })),
            );
            if !premium {
                footer = footer.child(
                    Button::new("folder-limit-premium")
                        .label("Get Telegram Premium")
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.open_url(PREMIUM_URL);
                            this.folders.limit_box = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FolderLimit, window, cx);
                        })),
                );
            }
            finish_dialog(
                dialog,
                title,
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }

    // ------------------------------------------------------------------
    // Tag colour picker
    // ------------------------------------------------------------------

    /// A click on a tag colour (`-1` = No tag). Colours need Premium with
    /// folder tags on; anyone else gets the Premium notice.
    fn choose_folder_tag_color(&mut self, color_id: i32, cx: &mut Context<Self>) {
        let (premium, tags_enabled) = (
            self.is_premium(),
            self.session().is_some_and(|s| s.are_folder_tags_enabled),
        );
        if !tag_choice_allowed(premium, tags_enabled) {
            self.show_folder_limit(FolderLimitKind::Tags, cx);
            return;
        }
        if let Some(dialog) = self.folders.editor.as_mut() {
            dialog.editor.color_id = color_id;
        }
        cx.notify();
    }

    /// "Folder color in chat list": seven colours and No tag, with a
    /// preview of the tag. Hidden for a Premium account whose folder tags
    /// are off (nothing to show), like tdesktop.
    pub(super) fn folder_tag_picker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.folders.editor.as_ref()?;
        let premium = self.is_premium();
        let tags_enabled = self.session().is_some_and(|s| s.are_folder_tags_enabled);
        if !tag_picker_visible(premium, tags_enabled) {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let chosen = tag_shown_choice(dialog.editor.color_id, premium);
        let preview_name = {
            let name = dialog.editor.name.trim().to_string();
            if name.is_empty() {
                dialog.name_input.read(cx).value().trim().to_string()
            } else {
                name
            }
        };
        let preview = if chosen < 0 {
            div()
                .text_xs()
                .text_color(muted)
                .child("No Tag")
                .into_any_element()
        } else {
            tag_chip(
                if preview_name.is_empty() {
                    "FOLDER".to_string()
                } else {
                    preview_name.to_uppercase()
                },
                chosen,
            )
            .into_any_element()
        };
        let mut row = div().id("folder-tag-colors").flex().items_center().gap_2();
        for color_id in (0..TAG_COLOR_COUNT).chain(std::iter::once(-1)) {
            let selected = chosen == color_id;
            let fill = if color_id < 0 {
                muted.opacity(0.35)
            } else {
                peer_name_color(color_id)
            };
            row = row.child(
                div()
                    .id(("folder-tag-color", (color_id + 1) as usize))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(26.))
                    .rounded_full()
                    .cursor_pointer()
                    .border_2()
                    .border_color(if selected {
                        Hsla::from(accent())
                    } else {
                        transparent_black()
                    })
                    .role(Role::RadioButton)
                    .aria_selected(selected)
                    .aria_label(if color_id < 0 {
                        "No tag".to_string()
                    } else {
                        format!("Tag color {}", color_id + 1)
                    })
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_folder_tag_color(color_id, cx);
                    }))
                    .child(
                        div()
                            .size(px(18.))
                            .rounded_full()
                            .bg(fill)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(color_id < 0, |this| {
                                this.child(
                                    Icon::new(if premium { IconName::X } else { IconName::Lock })
                                        .size(px(11.))
                                        .text_color(muted),
                                )
                            }),
                    ),
            );
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(muted)
                                .child("Folder color in chat list"),
                        )
                        .child(preview),
                )
                .child(row)
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Choose a color for the tag of this folder."),
                )
                .into_any_element(),
        )
    }
}

crate::ui::shell::register_dialogs! {
    /// A folder limit box / the folder tag Premium notice.
    FolderLimit => DialogSpec::new(
        1300,
        |app| app.folders.limit_box.is_some(),
        QuillApp::build_folder_limit_dialog,
    ),

    /// The shared folder's "N new chats" join dialog.
    FolderNewChats => DialogSpec::new(
        2200,
        |app| app.folders.new_chats_dialog.is_some(),
        QuillApp::build_folder_new_chats_dialog,
    ),
}

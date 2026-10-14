//! The sticker set box (Telegram Desktop `boxes/sticker_set_box.cpp`): a
//! set's stickers with Add/Remove, Share, Copy Link and Archive, and, for a
//! set the user created (`AmCreator`), editing after
//! `api/api_stickers_creator.cpp`: a "+" cell that adds a new image or an
//! existing sticker, drag reorder behind "Edit Stickers", a sticker's
//! "Delete sticker", and the ⋯ menu's "Edit name", "Reorder" and "Delete
//! Pack". New sets are made with the @stickers bot, as in tdesktop.
//! Decision: docs/decisions/sticker-set-creator.md.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{TestSupportExt, *};
use quill::ids::FileId;
use quill::state::StickerSetViewStage;
use quill::sticker_set_box::{
    ARCHIVED_NOTE, SetKind, can_archive, copied_note, has_add_cell, set_link, share_label,
};
use quill::telegram::envelope::StickerItem;
use std::cell::RefCell;
use std::rc::Rc;

mod creator;
mod demo;
mod dialogs;
mod menu;
pub(crate) mod state;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

/// A sticker being dragged to a new place in reorder mode.
#[derive(Clone)]
struct StickerCellDrag {
    set_id: i64,
    file_id: FileId,
    still: Option<std::path::PathBuf>,
}

impl Render for StickerCellDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size(px(72.))
            .p_1()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.25))
            .child(match self.still.clone() {
                Some(path) => img(path)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div().into_any_element(),
            })
    }
}

impl QuillApp {
    /// "View Sticker Set" / "Add Stickers": the set in a dialog.
    pub(in crate::ui) fn view_message_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        self.sticker_box.reset();
        self.message_ui.menu_ui.sticker_set_open = true;
        if let Some(live) = self.live.as_mut()
            && live.driver.view_sticker_set(set_id).is_err()
        {
            self.message_ui.menu_ui.sticker_set_open = false;
            self.connection.status_note = "could not open the sticker set".into();
        }
        cx.notify();
    }

    /// Install or remove the set shown in the sticker set dialog.
    pub(super) fn toggle_viewed_sticker_set(
        &mut self,
        set_id: i64,
        install: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.manage_sticker_set(set_id, install, false);
            if result.is_ok() {
                // The dialog stays; the set's state follows TDLib's update.
                live.driver.session.stickers.sticker_set_view = None;
                self.message_ui.menu_ui.sticker_set_open = false;
                self.connection.status_note = if install {
                    "sticker set added".into()
                } else {
                    "sticker set removed".into()
                };
            } else {
                self.connection.status_note = "could not change the sticker set".into();
            }
        } else {
            self.connection.status_note = "demo — sticker sets change with live TDLib".into();
        }
        cx.notify();
    }

    /// The box menu's "Share Stickers": the link goes to the chat
    /// chooser and lands in that chat's composer, unsent (tdesktop
    /// `FastShareLink`).
    pub(super) fn share_sticker_set_link(
        &mut self,
        link: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_sticker_set_dialog(cx);
        self.close_kit_dialog_if_done(DialogKind::StickerSet, window, cx);
        self.share.link_text = Some(link);
        self.connection.status_note = "choose a chat to share to".into();
        cx.notify();
    }

    /// The box menu's "Copy Link".
    pub(super) fn copy_sticker_set_link(
        &mut self,
        link: String,
        kind: SetKind,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(link));
        self.connection.status_note = copied_note(kind).into();
        cx.notify();
    }

    /// The box menu's "Archive Stickers": the set leaves the installed
    /// list and the box closes (tdesktop `archiveStickers`).
    pub(super) fn archive_viewed_sticker_set(
        &mut self,
        set_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.manage_sticker_set(set_id, false, true) {
                Ok(_) => {
                    live.driver.session.stickers.sticker_set_view = None;
                    self.message_ui.menu_ui.sticker_set_open = false;
                    self.connection.status_note = ARCHIVED_NOTE.into();
                    self.close_kit_dialog_if_done(DialogKind::StickerSet, window, cx);
                }
                Err(_) => self.connection.status_note = "could not archive the sticker set".into(),
            }
        } else {
            self.connection.status_note = "demo: sticker sets archive with live TDLib".into();
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_sticker_set_dialog(&mut self, cx: &mut Context<Self>) {
        self.message_ui.menu_ui.sticker_set_open = false;
        self.sticker_box.reset();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.sticker_set_view = None;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.sticker_set_view = None;
        }
        cx.notify();
    }

    /// The open set, if its stickers have arrived.
    fn viewed_set(&self) -> Option<(i64, SetKind, bool, Vec<StickerItem>)> {
        let view = self.session()?.stickers.sticker_set_view.as_ref()?;
        match &view.stage {
            StickerSetViewStage::Ready {
                is_emoji,
                is_owned,
                stickers,
                ..
            } => Some((
                view.set_id,
                SetKind::of(*is_emoji),
                *is_owned,
                stickers.clone(),
            )),
            _ => None,
        }
    }

    /// One sticker cell of the box: the still picture; in reorder mode a
    /// drag source and drop target; otherwise a right-click menu
    /// (favorites, "Delete sticker" for the creator, "Add to ... Set" for
    /// a sticker in someone else's set).
    fn sticker_set_cell(
        &self,
        set_id: i64,
        kind: SetKind,
        is_owned: bool,
        index: usize,
        item: &StickerItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let still = self.panel_still(item);
        let reordering = self.sticker_box.reordering;
        let file_id = item.file_id;
        let cell = div()
            .id(("sticker-set-cell", index as u64))
            .test_support()
            .size(px(72.))
            .p_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .hover(|style| style.bg(cx.theme().accent))
            .child(match still.clone() {
                Some(path) => img(path)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .size_full()
                    .rounded_md()
                    .bg(cx.theme().muted.opacity(0.5))
                    .into_any_element(),
            });
        if reordering {
            return cell
                .cursor_move()
                .on_drag(
                    StickerCellDrag {
                        set_id,
                        file_id,
                        still,
                    },
                    |drag: &StickerCellDrag, _, _, cx| cx.new(|_| drag.clone()),
                )
                .on_drop(cx.listener(move |this, drag: &StickerCellDrag, _, cx| {
                    if drag.set_id == set_id {
                        this.drop_sticker_in_set(set_id, drag.file_id, index, cx);
                    }
                }))
                .into_any_element();
        }
        let favorite = kind == SetKind::Stickers
            && self.session().is_some_and(|s| {
                s.stickers
                    .stickers
                    .favorites
                    .iter()
                    .any(|f| f.file_id == file_id)
            });
        // In someone else's set, a cell offers the owned sets of the same
        // kind (tdesktop `AddAddToStickerSetAction` / `AddAddToEmojiSetAction`
        // with no size filter; that filter is for set-less stickers).
        let owned_targets: Vec<(i64, String)> = if is_owned {
            Vec::new()
        } else {
            self.session()
                .map(|s| s.owned_sticker_sets(kind == SetKind::Emoji))
                .unwrap_or_default()
                .into_iter()
                .filter(|(id, _)| *id != set_id)
                .collect()
        };
        let item = item.clone();
        let owner = cx.entity().downgrade();
        cell.context_menu(move |menu, window, cx| {
            let mut menu = menu;
            if kind == SetKind::Stickers {
                let favorite_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(if favorite {
                        "Remove from Favorites"
                    } else {
                        "Add to Favorites"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = favorite_owner.update(cx, |this, cx| {
                            this.favorite_message_sticker(file_id, !favorite, cx);
                        });
                    }),
                );
            }
            if is_owned {
                let delete_owner = owner.clone();
                let item = item.clone();
                menu = menu.item(PopupMenuItem::new(kind.delete_label()).on_click(
                    move |_, _, cx| {
                        let item = item.clone();
                        let _ = delete_owner.update(cx, |this, cx| {
                            this.ask_delete_sticker(item, cx);
                        });
                    },
                ));
            } else if !owned_targets.is_empty() {
                let targets = owned_targets.clone();
                let item = item.clone();
                let add_owner = owner.clone();
                menu = menu.submenu(kind.add_to_set_label(), window, cx, move |sub, _, _| {
                    let mut sub = sub;
                    for (target_id, title) in targets.clone() {
                        let item = item.clone();
                        let add_owner = add_owner.clone();
                        sub = sub.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                            let item = item.clone();
                            let _ = add_owner.update(cx, |this, cx| {
                                this.add_existing_to_owned_set(target_id, kind, &item, cx);
                            });
                        }));
                    }
                    sub
                });
            }
            menu
        })
        .into_any_element()
    }

    /// The sticker set dialog: its stickers, and Add/Remove.
    pub(super) fn build_sticker_set_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerSet, |this, _, cx| {
                this.close_sticker_set_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let view = this
                .session()
                .and_then(|s| s.stickers.sticker_set_view.clone());
            // The previews download once the set has arrived.
            if let Some(live) = this.live.as_mut() {
                let _ = live.driver.ensure_sticker_set_view_files();
            }
            let dialog = dialog.overlay(true);
            let Some(view) = view else {
                return dialog
                    .title(shell::dialog_title("Sticker set"))
                    .on_close(on_close);
            };
            let (title, body, footer): (String, AnyElement, Option<AnyElement>) = match view.stage {
                StickerSetViewStage::Loading => (
                    "Sticker set".into(),
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Loading stickers…")
                        .into_any_element(),
                    None,
                ),
                StickerSetViewStage::Failed => (
                    "Sticker set".into(),
                    div()
                        .text_sm()
                        .text_color(danger())
                        .child("Couldn't load this sticker set.")
                        .into_any_element(),
                    None,
                ),
                StickerSetViewStage::Ready {
                    title,
                    name,
                    installed,
                    is_emoji,
                    is_owned,
                    stickers,
                } => {
                    let set_id = view.set_id;
                    let kind = SetKind::of(is_emoji);
                    let count = stickers.len();
                    let reordering = this.sticker_box.reordering;
                    let mut grid = div().flex().flex_wrap().gap_1().w(px(380.));
                    for (index, item) in stickers.iter().enumerate() {
                        grid = grid
                            .child(this.sticker_set_cell(set_id, kind, is_owned, index, item, cx));
                    }
                    if !reordering && has_add_cell(is_owned, count, kind) {
                        grid = grid.child(this.sticker_set_add_cell(set_id, kind, cx));
                    }
                    let more: Option<AnyElement> = set_link(&name, kind).map(|link| {
                        this.sticker_set_more_button(
                            set_id,
                            kind,
                            link,
                            can_archive(installed, kind),
                            is_owned,
                            cx,
                        )
                        .into_any_element()
                    });
                    let footer = if reordering {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1())
                            .child(
                                Button::new("sticker-set-reorder-done")
                                    .label("Done")
                                    .primary()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.finish_edit_stickers(cx);
                                    })),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .children(more)
                            .child(div().flex_1())
                            .child(
                                Button::new("sticker-set-close")
                                    .label("Close")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_sticker_set_dialog(cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::StickerSet,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .map(|footer| {
                                // tdesktop: a set that is not installed gets
                                // "Add N Stickers"; an installed set the user
                                // created gets "Edit Stickers" in its place;
                                // any other installed set "Remove Stickers".
                                if installed && is_owned {
                                    footer.child(
                                        Button::new("sticker-set-edit")
                                            .label(kind.edit_label())
                                            .primary()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.begin_edit_stickers(cx);
                                            })),
                                    )
                                } else {
                                    footer.child(
                                        Button::new("sticker-set-toggle")
                                            .label(if installed {
                                                "Remove Stickers".to_string()
                                            } else {
                                                format!("Add {count} Stickers")
                                            })
                                            .when(installed, |button| button.danger())
                                            .when(!installed, |button| button.primary())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.toggle_viewed_sticker_set(
                                                    set_id, !installed, cx,
                                                );
                                                this.close_kit_dialog_if_done(
                                                    DialogKind::StickerSet,
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    )
                                }
                            })
                            .into_any_element()
                    };
                    (title, grid.into_any_element(), Some(footer))
                }
            };
            let dialog =
                dialog
                    .title(shell::dialog_title(title))
                    .content(shell::scrollable_dialog_content({
                        let body = Rc::new(RefCell::new(Some(body)));
                        move |content, _, _| {
                            let body = body
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element());
                            content.child(body)
                        }
                    }));
            match footer {
                Some(footer) => dialog.footer(footer),
                None => dialog,
            }
            .on_close(on_close)
        })
    }
}

/// The set box and its sub-dialogs show while the box is open and the
/// photo editor is not: the editor ("Create a New Sticker", "Adapt from a
/// Sticker") paints inside the app, under any kit dialog, so the box steps
/// aside until the editor is done or cancelled.
pub(super) fn sticker_box_shown(app: &QuillApp) -> bool {
    app.message_ui.menu_ui.sticker_set_open && app.viewer.photo_editor.is_none()
}

crate::ui::shell::register_dialogs! {
    /// "View Sticker Set" / "Add Stickers" from a sticker message.
    StickerSet => DialogSpec::new(
        5700,
        sticker_box_shown,
        QuillApp::build_sticker_set_dialog,
    ),
}

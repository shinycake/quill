//! The set box's secondary boxes: "Edit Sticker Set Name", "Delete
//! sticker", "Delete for Everyone" / "Delete for Myself" confirmations, the
//! "New Sticker" emoji step and the existing-sticker chooser. Each is a kit
//! dialog registered below; a lower priority than the set box means it
//! shows on top of it and the box returns when it closes.
use super::*;
use gpui_kit::component::input::Input;
use quill::sticker_set_box::{MAX_EMOJIS, SET_NAME_MAX};

/// The emoji grid of the "New Sticker" box: the catalog filtered by the
/// search field, the first screen of smileys when it is empty.
const EMOJI_GRID: usize = 48;

impl QuillApp {
    fn viewed_title(&self) -> String {
        self.session()
            .and_then(|s| s.stickers.sticker_set_view.as_ref())
            .and_then(|view| match &view.stage {
                StickerSetViewStage::Ready { title, .. } => Some(title.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// `lng_stickers_context_edit_name` box.
    pub(super) fn build_rename_set_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerSetRename, |this, _, cx| {
                this.sticker_box.rename_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let set_id = this.viewed_set().map(|(id, ..)| id).unwrap_or(0);
            let input = this.sticker_box.rename_input.clone();
            let length = input.read(cx).value().chars().count();
            let busy = this
                .session()
                .and_then(|s| s.stickers.sticker_set_view.as_ref())
                .is_some_and(|view| view.edits.busy());
            dialog
                .overlay(true)
                .w(px(380.))
                .title(shell::dialog_title("Edit Sticker Set Name"))
                .content(move |content, _, _| {
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(text_muted())
                                    .child("Choose a name for your set."),
                            )
                            .child(Input::new(&input))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted())
                                    .child(format!("{length}/{SET_NAME_MAX}")),
                            ),
                    )
                })
                .footer(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("sticker-set-rename-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sticker_box.rename_open = false;
                                    this.close_kit_dialog_if_done(
                                        DialogKind::StickerSetRename,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("sticker-set-rename-done")
                                .label("Done")
                                .primary()
                                .loading(busy)
                                .disabled(length == 0 || length > SET_NAME_MAX)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.confirm_rename_set(set_id, cx) {
                                        this.close_kit_dialog_if_done(
                                            DialogKind::StickerSetRename,
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        ),
                )
                .on_close(on_close)
        })
    }

    /// `lng_stickers_context_delete_sure` box with the sticker's picture.
    pub(super) fn build_delete_sticker_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerDelete, |this, _, cx| {
                this.sticker_box.delete_sticker = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let (set_id, kind) = this
                .viewed_set()
                .map(|(id, kind, ..)| (id, kind))
                .unwrap_or((0, SetKind::Stickers));
            let still = this
                .sticker_box
                .delete_sticker
                .as_ref()
                .and_then(|item| this.panel_still(item));
            let busy = this
                .session()
                .and_then(|s| s.stickers.sticker_set_view.as_ref())
                .is_some_and(|view| view.edits.busy());
            let title = match kind {
                SetKind::Stickers => "Delete sticker",
                SetKind::Emoji => "Delete emoji",
            };
            dialog
                .overlay(true)
                .w(px(360.))
                .title(shell::dialog_title(title))
                .content(move |content, _, _| {
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .children(
                                still.clone().map(|path| {
                                    img(path).size(px(96.)).object_fit(ObjectFit::Contain)
                                }),
                            )
                            .child(div().text_sm().child(kind.delete_question())),
                    )
                })
                .footer(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("sticker-delete-close")
                                .label("Close")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sticker_box.delete_sticker = None;
                                    this.close_kit_dialog_if_done(
                                        DialogKind::StickerDelete,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("sticker-delete-confirm")
                                .label("Delete")
                                .danger()
                                .loading(busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_delete_sticker(set_id, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::StickerDelete,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .on_close(on_close)
        })
    }

    /// "Delete for Everyone" (`lng_stickers_delete_pack_everyone_sure`).
    pub(super) fn build_delete_set_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerSetDelete, |this, _, cx| {
                this.sticker_box.delete_set_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let set_id = this.viewed_set().map(|(id, ..)| id).unwrap_or(0);
            confirm_dialog(
                dialog,
                "Delete Pack",
                "Are you sure you want to delete this sticker set for everyone? This cannot be undone.",
                "sticker-set-delete",
                cx,
                move |this, window, cx| {
                    this.confirm_delete_set_everyone(set_id, cx);
                    this.close_kit_dialog_if_done(DialogKind::StickerSetDelete, window, cx);
                },
                |this, window, cx| {
                    this.sticker_box.delete_set_open = false;
                    this.close_kit_dialog_if_done(DialogKind::StickerSetDelete, window, cx);
                    cx.notify();
                },
            )
            .on_close(on_close)
        })
    }

    /// "Delete for Myself" (`MakeConfirmRemoveSetBox`).
    pub(super) fn build_remove_set_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerSetRemove, |this, _, cx| {
                this.sticker_box.remove_set_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let set_id = this.viewed_set().map(|(id, ..)| id).unwrap_or(0);
            let title = this.viewed_title();
            confirm_dialog(
                dialog,
                "Remove stickers",
                format!("Remove \"{title}\"?"),
                "sticker-set-remove",
                cx,
                move |this, window, cx| {
                    this.confirm_remove_set(set_id, cx);
                    this.close_kit_dialog_if_done(DialogKind::StickerSetRemove, window, cx);
                },
                |this, window, cx| {
                    this.sticker_box.remove_set_open = false;
                    this.close_kit_dialog_if_done(DialogKind::StickerSetRemove, window, cx);
                    cx.notify();
                },
            )
            .on_close(on_close)
        })
    }

    /// The "New Sticker" / "New Emoji" box: the image and its emoji.
    pub(super) fn build_create_sticker_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerCreate, |this, _, cx| {
                this.sticker_box.create = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(create) = this.sticker_box.create.clone() else {
                return dialog
                    .title(shell::dialog_title("New Sticker"))
                    .on_close(on_close);
            };
            let kind = create.kind;
            let preview = this.create_preview_path();
            let search = this.sticker_box.emoji_search.clone();
            let query = search.read(cx).value().trim().to_string();
            let chosen = create.emojis.clone();
            let results: Vec<&'static str> = if query.is_empty() {
                quill::emoji_catalog::search(Some("Smileys & Emotion"), "")
                    .map(|entry| entry.emoji)
                    .take(EMOJI_GRID)
                    .collect()
            } else {
                quill::emoji_catalog::search(None, &query)
                    .map(|entry| entry.emoji)
                    .take(EMOJI_GRID)
                    .collect()
            };
            let chosen_row = div().flex().flex_wrap().gap_1().min_h(px(36.)).children(
                chosen.iter().enumerate().map(|(index, emoji)| {
                    let emoji = emoji.clone();
                    Button::new(("sticker-create-chosen", index as u64))
                        .label(emoji.clone())
                        .outline()
                        .small()
                        .tooltip("Remove")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_create_emoji(&emoji, cx);
                        }))
                }),
            );
            let grid =
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(results.into_iter().enumerate().map(|(index, emoji)| {
                        let picked = chosen.iter().any(|e| e == emoji);
                        div()
                            .id(("sticker-create-emoji", index as u64))
                            .size(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .text_xl()
                            .cursor_pointer()
                            .when(picked, |cell| cell.bg(cx.theme().accent))
                            .hover(|style| style.bg(cx.theme().accent))
                            .child(emoji)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_create_emoji(emoji, cx);
                            }))
                    }));
            let about = format!("{} ({}/{MAX_EMOJIS})", kind.emoji_about(), chosen.len());
            let body = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .children(
                    preview.map(|path| img(path).size(px(256.)).object_fit(ObjectFit::Contain)),
                )
                .child(div().text_sm().text_color(text_muted()).child(about))
                .child(Input::new(&search))
                .child(chosen_row)
                .child(grid)
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .w(px(420.))
                .title(shell::dialog_title(kind.create_title()))
                .content(shell::scrollable_dialog_content(move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                }))
                .footer(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("sticker-create-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.sticker_box.create = None;
                                    this.close_kit_dialog_if_done(
                                        DialogKind::StickerCreate,
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("sticker-create-done")
                                .label("Done")
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.confirm_create_sticker(cx) {
                                        this.close_kit_dialog_if_done(
                                            DialogKind::StickerCreate,
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        ),
                )
                .on_close(on_close)
        })
    }

    /// The existing-sticker chooser (tdesktop opens the sticker panel).
    pub(super) fn build_pick_sticker_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StickerPick, |this, _, cx| {
                this.sticker_box.pick = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(pick) = this.sticker_box.pick else {
                return dialog
                    .title(shell::dialog_title("Choose a sticker"))
                    .on_close(on_close);
            };
            let title = if pick.adapt {
                "Adapt from a Sticker"
            } else {
                match pick.kind {
                    SetKind::Stickers => "Add an Existing Sticker",
                    SetKind::Emoji => "Add an Existing Emoji",
                }
            };
            let candidates = this.pick_candidates(pick);
            let cells: Vec<AnyElement> = candidates
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let still = this.panel_still(item);
                    let item = item.clone();
                    div()
                        .id(("sticker-pick-cell", index as u64))
                        .size(px(64.))
                        .p_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().accent))
                        .child(match still {
                            Some(path) => img(path)
                                .size_full()
                                .object_fit(ObjectFit::Contain)
                                .into_any_element(),
                            None => div()
                                .size_full()
                                .rounded_md()
                                .bg(cx.theme().muted.opacity(0.5))
                                .into_any_element(),
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_existing_sticker(&item, cx);
                            this.close_kit_dialog_if_done(DialogKind::StickerPick, window, cx);
                        }))
                        .into_any_element()
                })
                .collect();
            let empty = cells.is_empty();
            let body = Rc::new(RefCell::new(Some(cells)));
            dialog
                .overlay(true)
                .w(px(400.))
                .title(shell::dialog_title(title))
                .content(shell::scrollable_dialog_content(move |content, _, _| {
                    let cells = body.borrow_mut().take().unwrap_or_default();
                    content.child(if empty {
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child("Open the sticker panel once to load stickers to choose from.")
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .children(cells)
                            .into_any_element()
                    })
                }))
                .footer(
                    div().flex().justify_end().child(
                        Button::new("sticker-pick-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.sticker_box.pick = None;
                                this.close_kit_dialog_if_done(DialogKind::StickerPick, window, cx);
                                cx.notify();
                            })),
                    ),
                )
                .on_close(on_close)
        })
    }
}

/// A two-button confirmation: text, "Cancel", and a red "Remove"
/// (tdesktop's attention button).
fn confirm_dialog(
    dialog: Dialog,
    title: &'static str,
    text: impl Into<SharedString>,
    id: &'static str,
    cx: &mut Context<QuillApp>,
    confirm: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
    cancel: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> Dialog {
    let text: SharedString = text.into();
    dialog
        .overlay(true)
        .w(px(360.))
        .title(shell::dialog_title(title))
        .content(move |content, _, _| content.child(div().text_sm().child(text.clone())))
        .footer(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    Button::new(format!("{id}-cancel"))
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| cancel(this, window, cx))),
                )
                .child(
                    Button::new(format!("{id}-confirm"))
                        .label("Remove")
                        .danger()
                        .on_click(
                            cx.listener(move |this, _, window, cx| confirm(this, window, cx)),
                        ),
                ),
        )
}

crate::ui::shell::register_dialogs! {
    /// "Edit name" in the set box's menu.
    StickerSetRename => DialogSpec::new(
        5610,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.rename_open,
        QuillApp::build_rename_set_dialog,
    ),
    /// A cell's "Delete sticker" / "Delete emoji".
    StickerDelete => DialogSpec::new(
        5620,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.delete_sticker.is_some(),
        QuillApp::build_delete_sticker_dialog,
    ),
    /// "Delete Pack" → "Delete for Everyone".
    StickerSetDelete => DialogSpec::new(
        5630,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.delete_set_open,
        QuillApp::build_delete_set_dialog,
    ),
    /// "Delete Pack" → "Delete for Myself".
    StickerSetRemove => DialogSpec::new(
        5640,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.remove_set_open,
        QuillApp::build_remove_set_dialog,
    ),
    /// The "New Sticker" / "New Emoji" emoji step.
    StickerCreate => DialogSpec::new(
        5650,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.create.is_some(),
        QuillApp::build_create_sticker_dialog,
    ),
    /// "Add an Existing Sticker" / "Adapt from a Sticker".
    StickerPick => DialogSpec::new(
        5660,
        |app| app.message_ui.menu_ui.sticker_set_open && app.sticker_box.pick.is_some(),
        QuillApp::build_pick_sticker_dialog,
    ),
}

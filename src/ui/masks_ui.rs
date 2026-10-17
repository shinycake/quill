//! Mask sticker sets: the manage dialog (Masks and Archived tabs, reorder,
//! archive, remove) and the Masks strip in the photo editor.
//!
//! tdesktop only offers masks where an image is edited: the story editor's
//! sticker panel has a Masks tab, and its gear opens the same box as the
//! sticker manager with just Masks and Archived tabs. Quill mirrors that in
//! the photo editor's Stickers mode. A mask sticker is placed as a plain
//! picture; neither app fits it to a face.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::state::{MasksTab, StickersPurpose};
use quill::telegram::envelope::StickerSetInfo;
use std::cell::RefCell;
use std::rc::Rc;

/// Mask thumbnails shown in the editor strip.
const STRIP_LIMIT: usize = 24;

/// "1 mask", "3 masks" (tdesktop `lng_masks_count`).
fn masks_count_label(count: i32) -> String {
    if count == 1 {
        tr(keys::Q_MISC_1_MASK).into()
    } else {
        format!("{count} masks")
    }
}

impl QuillApp {
    fn masks_panel_mut(&mut self) -> Option<&mut quill::state::MasksPanel> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.stickers.masks)
        } else {
            self.demo_session
                .as_mut()
                .map(|session| &mut session.stickers.masks)
        }
    }

    pub(super) fn open_mask_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_mask_sets().is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS)));
            }
        } else if let Some(masks) = self.masks_panel_mut() {
            masks.open = true;
        }
        cx.notify();
    }

    fn close_mask_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(masks) = self.masks_panel_mut() {
            masks.open = false;
            masks.confirm_remove = None;
        }
        cx.notify();
    }

    fn select_mask_tab(&mut self, tab: MasksTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_mask_tab(tab).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS)));
            }
        } else if let Some(masks) = self.masks_panel_mut() {
            masks.tab = tab;
        }
        if let Some(masks) = self.masks_panel_mut() {
            masks.confirm_remove = None;
        }
        cx.notify();
    }

    fn change_mask_set(
        &mut self,
        set_id: i64,
        installed: bool,
        archived: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(masks) = self.masks_panel_mut() {
            masks.confirm_remove = None;
        }
        if let Some(live) = self.live.as_mut() {
            self.connection.set_note(
                match live.driver.change_mask_set(set_id, installed, archived) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MISC_UPDATING_MASKS)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MISC_A_MASK_UPDATE_IS_ALREADY)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_MASKS)),
                },
            );
        }
        cx.notify();
    }

    fn reorder_mask_set(&mut self, source: i64, target: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.reorder_mask_set(source, target) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MISC_SAVING_MASK_ORDER)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MISC_MASK_ORDER_UNCHANGED_OR_AN)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_REORDER_MASKS)),
                });
        }
        cx.notify();
    }

    fn mask_row(
        &self,
        set: &StickerSetInfo,
        tab: MasksTab,
        busy: bool,
        confirming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let set_id = set.id;
        let title = if set.title.is_empty() {
            set.name.clone()
        } else {
            set.title.clone()
        };
        let mut actions = div().flex().items_center().gap_1();
        match tab {
            MasksTab::Installed if confirming => {
                actions = actions
                    .child(
                        Button::new(format!("mask-remove-confirm-{set_id}"))
                            .label(tr(keys::BOX_REMOVE))
                            .small()
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change_mask_set(set_id, false, false, cx)
                            })),
                    )
                    .child(
                        Button::new(format!("mask-remove-cancel-{set_id}"))
                            .label(tr(keys::CANCEL))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(masks) = this.masks_panel_mut() {
                                    masks.confirm_remove = None;
                                }
                                cx.notify();
                            })),
                    );
            }
            MasksTab::Installed => {
                actions = actions
                    .child(
                        Button::new(format!("mask-archive-{set_id}"))
                            .label(tr(keys::Q_MISC_ARCHIVE))
                            .small()
                            .ghost()
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change_mask_set(set_id, false, true, cx)
                            })),
                    )
                    .child(
                        Button::new(format!("mask-remove-{set_id}"))
                            .label(tr(keys::BOX_REMOVE))
                            .small()
                            .ghost()
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(masks) = this.masks_panel_mut() {
                                    masks.confirm_remove = Some(set_id);
                                }
                                cx.notify();
                            })),
                    );
            }
            MasksTab::Archived => {
                actions = actions.child(
                    Button::new(format!("mask-restore-{set_id}"))
                        .label(tr(keys::Q_MISC_RESTORE))
                        .small()
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_mask_set(set_id, true, false, cx)
                        })),
                );
            }
        }
        let mut row = div()
            .id(format!("mask-set-row-{set_id}"))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().font_semibold().truncate().child(title.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if set.size == 0 {
                                tr(keys::Q_MISC_LOADING).to_string()
                            } else {
                                masks_count_label(set.size)
                            }),
                    ),
            )
            .child(actions);
        if tab == MasksTab::Installed {
            row = row
                .cursor_move()
                .on_drag(
                    MaskSetDrag { id: set_id, title },
                    |drag: &MaskSetDrag, _, _, cx| cx.new(|_| drag.clone()),
                )
                .on_drop(cx.listener(move |this, drag: &MaskSetDrag, _, cx| {
                    this.reorder_mask_set(drag.id, set_id, cx);
                }));
        }
        row.into_any_element()
    }

    fn mask_sets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let panel = session
            .map(|s| s.stickers.masks.clone())
            .unwrap_or_default();
        let busy = session.is_some_and(|s| s.mask_change_pending());
        let loading = session.is_some_and(|s| {
            s.requests
                .has_purpose(StickersPurpose::GetInstalledMaskSets.into())
                || s.requests
                    .has_purpose(StickersPurpose::GetArchivedMaskSets { first_page: true }.into())
                || s.requests
                    .has_purpose(StickersPurpose::GetArchivedMaskSets { first_page: false }.into())
        });
        let mut tabs = div().flex().gap_2();
        for (title, tab) in [
            (tr(keys::SWITCH_MASKS), MasksTab::Installed),
            (tr(keys::Q_MISC_ARCHIVED), MasksTab::Archived),
        ] {
            tabs = tabs.child(
                Button::new(format!("mask-sets-tab-{title}"))
                    .label(title)
                    .selected(panel.tab == tab)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| this.select_mask_tab(tab, cx))),
            );
        }
        let (sets, loaded) = match panel.tab {
            MasksTab::Installed => (&panel.installed, panel.installed_loaded),
            MasksTab::Archived => (&panel.archived, panel.archived_loaded),
        };
        let mut rows = div().id("mask-set-rows").flex().flex_col().gap_1();
        for set in sets {
            rows = rows.child(self.mask_row(
                set,
                panel.tab,
                busy,
                panel.confirm_remove == Some(set.id),
                cx,
            ));
        }
        if panel.tab == MasksTab::Archived && panel.archived_has_more {
            rows = rows.child(
                Button::new("more-archived-masks")
                    .label(tr(keys::Q_MISC_MORE_ARCHIVED_MASKS))
                    .ghost()
                    .disabled(loading)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut()
                            && live.driver.more_archived_masks().is_err()
                        {
                            this.connection.set_note(StatusNote::error(tr(
                                keys::Q_MISC_COULD_NOT_LOAD_MORE_MASKS,
                            )));
                        }
                        cx.notify();
                    })),
            );
        }
        let status = if panel.failed {
            Some(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS_SWITCH))
        } else if busy {
            Some(tr(keys::Q_MISC_UPDATING_MASKS))
        } else if loading || !loaded {
            Some(tr(keys::Q_MISC_LOADING_MASKS))
        } else if sets.is_empty() {
            Some(match panel.tab {
                MasksTab::Installed => tr(keys::Q_MISC_NO_MASKS_YET_ADD_SOME),
                MasksTab::Archived => tr(keys::Q_MISC_NO_ARCHIVED_MASKS),
            })
        } else {
            None
        };
        div()
            .id("mask-sets-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .child(tabs)
            .children(status.map(|text| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text)
            }))
            .child(rows)
            .into_any_element()
    }

    pub(super) fn build_mask_sets_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close = QuillShell::on_close_kind(app, shell, DialogKind::MaskSets, |this, _, cx| {
            this.close_mask_sets(cx)
        });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(this.mask_sets_panel(cx))));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::SWITCH_MASKS)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    },
                ))
                .on_close(close)
        })
    }

    /// The Masks row of the photo editor's Stickers mode: pictures from
    /// your installed mask sets, and a button for the manage dialog.
    pub(super) fn photo_editor_mask_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let roots = self.media_display_roots();
        let session = self.session();
        let panel = session
            .map(|s| s.stickers.masks.clone())
            .unwrap_or_default();
        let thumbs: Vec<_> = panel
            .installed
            .iter()
            .filter_map(|set| panel.set_stickers.get(&set.id))
            .flatten()
            .filter_map(|item| {
                let session = session?;
                let path = item
                    .display_file_id()
                    .into_iter()
                    .chain(item.thumb_file_id)
                    .find_map(|id| session.media.files.get(&id.0)?.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))?;
                Some((item.file_id, item.format, path))
            })
            .take(STRIP_LIMIT)
            .collect();
        let empty_note = if panel.failed {
            Some(tr(keys::Q_MISC_COULD_NOT_LOAD_MASKS))
        } else if panel.installed_loaded && panel.installed.is_empty() {
            Some(tr(keys::Q_MISC_NO_MASKS_INSTALLED))
        } else {
            None
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .child(tr(keys::SWITCH_MASKS)),
                    )
                    .child(
                        Button::new("photo-editor-manage-masks")
                            .label(tr(keys::Q_MISC_MANAGE_MASKS))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.open_mask_sets(cx))),
                    ),
            )
            .children(empty_note.map(|text| div().text_xs().child(text)))
            .when(!thumbs.is_empty(), |this| {
                this.child(div().flex().flex_wrap().justify_center().gap_1().children(
                    thumbs.into_iter().map(|(file_id, format, thumb)| {
                        div()
                            .id(("photo-editor-mask", file_id.0 as u64))
                            .size(px(44.))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|style| style.bg(gpui_kit::white().opacity(0.12)))
                            .child(img(thumb).size_full().object_fit(ObjectFit::Contain))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.place_editor_sticker(file_id, format, cx);
                            }))
                    }),
                ))
            })
            .into_any_element()
    }
}

#[derive(Clone)]
struct MaskSetDrag {
    id: i64,
    title: String,
}

impl Render for MaskSetDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.title.clone())
    }
}

crate::ui::shell::register_dialogs! {
    MaskSets => DialogSpec::new(
        5110,
        |app| app.session().is_some_and(|s| s.stickers.masks.open),
        QuillApp::build_mask_sets_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::masks_count_label;

    #[test]
    fn counts_are_singular_for_one() {
        assert_eq!(masks_count_label(1), "1 mask");
        assert_eq!(masks_count_label(0), "0 masks");
        assert_eq!(masks_count_label(12), "12 masks");
    }
}

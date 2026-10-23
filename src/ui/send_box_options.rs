//! The send box's media options (tdesktop `SendFilesBox`): "High quality"
//! for big photos, the price of paid media in channels (the "Paid Content"
//! box), a video's cover and "Edit Video" on its card.
//!
//! The choices live in `SendBoxUi` (`send_box_state.rs`) and ride the
//! snapshot as `MediaSendOptions`; covers and edits ride the attachment
//! (`AttachmentExtras`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::{SendSlot, TrayItem};
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{
    AttachmentKind, ComposerAttachment, MediaSendOptions, PriceInput, can_use_high_quality,
    cover_send_path, is_gif_path, paid_media_fits, parse_paid_price, session_allows_cover,
    session_allows_paid_media,
};
use quill::lang::{keys, tr, tr_with};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// The "Paid Content" box (tdesktop `EditPriceBox`).
pub(super) struct PriceDialog {
    pub(super) input: Entity<InputState>,
    /// The typed price is over the limit (`field->showError()`).
    pub(super) too_high: bool,
    /// The chat the price is for: the one open when the box opened.
    pub(super) slot: SendSlot,
}

impl QuillApp {
    /// The chat the tray sends to is a channel that takes paid media.
    fn open_chat_takes_paid_media(&self) -> bool {
        self.session().is_some_and(|session| {
            session
                .open_chat
                .is_some_and(|chat| session_allows_paid_media(session, chat))
        })
    }

    /// The photos and videos in the tray can go out as paid media.
    pub(super) fn paid_media_available(&self) -> bool {
        self.open_chat_takes_paid_media()
            && paid_media_fits(
                self.pending_attachments(),
                self.composer_group_media_effective(),
            )
    }

    /// The dearest price the account may set (the TDLib option).
    fn paid_media_star_max(&self) -> i64 {
        self.session()
            .map_or(quill::composer::PAID_MEDIA_MAX_STARS, |session| {
                session.messages.paid_media_star_max
            })
    }

    /// The price in effect for the next send; 0 when paid media is not
    /// available for what is in the tray.
    pub(super) fn paid_stars_in_effect(&self) -> i64 {
        if self.paid_media_available() {
            self.pending_send().paid_stars
        } else {
            0
        }
    }

    /// Send-box choices for the snapshot of the next send.
    pub(super) fn composer_media_options(&self) -> MediaSendOptions {
        let as_files = self
            .pending_attachments()
            .iter()
            .all(|attachment| attachment.kind == AttachmentKind::Document);
        MediaSendOptions {
            hd_photos: self.pending_send().hd_photos && !as_files,
            paid_stars: self.paid_stars_in_effect(),
        }
    }

    /// Size from the file header, read once per photo.
    fn tray_photo_size(&self, path: &Path) -> Option<(u32, u32)> {
        *self
            .composer_ui
            .send_box
            .photo_sizes
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| image::image_dimensions(path).ok())
    }

    /// tdesktop `hasSendLargePhotosOption`: a compressed photo over 1280 px.
    pub(super) fn tray_has_large_photo(&self) -> bool {
        self.pending_attachments()
            .iter()
            .filter(|attachment| attachment.kind == AttachmentKind::Photo)
            .filter_map(|attachment| self.tray_photo_size(&attachment.path))
            .any(|(width, height)| can_use_high_quality(width, height))
    }

    /// Read the sound track of the new tray videos in the background; a
    /// video without one becomes a GIF when it lands (tdesktop
    /// `modifications.gif = !hasAudio`). Sending waits for the reads. Also
    /// starts the thumbnails of pictures sent as files.
    pub(super) fn check_tray_videos(&mut self, cx: &mut Context<Self>) {
        // Every tray, not just the open one: a picker can answer into
        // another chat's tray.
        let paths: Vec<std::path::PathBuf> = self
            .composer_sends
            .attachments()
            .filter(|attachment| attachment.needs_sound_check())
            .map(|attachment| attachment.path.clone())
            .filter(|path| {
                let send_box = &self.composer_ui.send_box;
                !send_box.sound_checking.contains(path) && !send_box.sound_checked.contains(path)
            })
            .collect();
        for path in paths {
            self.composer_ui
                .send_box
                .sound_checking
                .insert(path.clone());
            let probed = path.clone();
            let worker =
                cx.background_spawn(async move { quill::composer::without_sound_track(&probed) });
            self.spawn_scoped(cx, async move |this, cx| {
                let silent = worker.await;
                let _ = this.update(cx, |this, cx| {
                    let send_box = &mut this.composer_ui.send_box;
                    // The batch may have been sent or cleared meanwhile.
                    if !send_box.sound_checking.remove(&path) {
                        return;
                    }
                    send_box.sound_checked.insert(path.clone());
                    if silent {
                        for attachment in this
                            .composer_sends
                            .attachments_at_mut(&path)
                            .filter(|attachment| attachment.needs_sound_check())
                        {
                            attachment.extras.gif = true;
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        self.prepare_tray_file_thumbnails(cx);
    }

    /// Videos in the open slot's tray are still being read (a probe for
    /// another chat's tray does not count).
    pub(super) fn tray_videos_checking(&self) -> bool {
        let checking = &self.composer_ui.send_box.sound_checking;
        self.pending_attachments()
            .iter()
            .any(|attachment| checking.contains(&attachment.path))
    }

    /// The options row of the tray: High quality and the price.
    pub(super) fn send_box_option_controls(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut controls = Vec::new();
        if self.tray_videos_checking() {
            controls.push(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_CHECKING_THE_VIDEO))
                    .into_any_element(),
            );
        }
        if self.tray_preparing() {
            controls.push(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_OPS_PREPARING_THE_FILES))
                    .into_any_element(),
            );
        }
        if self.tray_has_large_photo() {
            controls.push(
                gpui_kit::component::checkbox::Checkbox::new("composer-high-quality")
                    .label(tr(keys::SEND_HIGH_QUALITY))
                    .checked(self.pending_send().hd_photos)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_pending_send(|send| send.hd_photos = !send.hd_photos);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if self.paid_media_available() {
            let stars = self.pending_send().paid_stars;
            controls.push(
                Button::new("composer-paid-media")
                    .icon(gpui_kit::assets::IconName::Star)
                    .label(if stars > 0 {
                        tr_with(
                            keys::Q_OPS_UNLOCK_FOR_STARS,
                            &[("stars", &stars.to_string())],
                        )
                    } else {
                        tr(keys::Q_OPS_MAKE_PAID).to_string()
                    })
                    .ghost()
                    .small()
                    .selected(stars > 0)
                    .tooltip(if stars > 0 {
                        tr(keys::Q_OPS_CHANGE_THE_PRICE)
                    } else {
                        tr(keys::Q_OPS_ASK_FOR_STARS_TO_SEE)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_price_dialog(window, cx);
                    }))
                    .into_any_element(),
            );
        }
        controls
    }

    /// Whether the tray hides the spoiler button: paid media is blurred by
    /// Telegram itself (tdesktop `hasPrice()` turns spoilers off).
    pub(super) fn tray_paid_price_set(&self) -> bool {
        self.paid_stars_in_effect() > 0
    }

    /// "Edit video" and "Cover" for a video card.
    pub(super) fn video_card_actions(
        &self,
        index: usize,
        attachment: &ComposerAttachment,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // A `.gif` file is an animation already: nothing to edit.
        if attachment.kind != AttachmentKind::Video || is_gif_path(&attachment.path) {
            return Vec::new();
        }
        let mut actions = vec![
            Button::new(("composer-attach-edit-video", index as u64))
                .icon(gpui_kit::assets::IconName::Pencil)
                .xsmall()
                .ghost()
                .tooltip(tr(keys::Q_OPS_EDIT_VIDEO))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_tray_video_editor(index, window, cx);
                }))
                .into_any_element(),
        ];
        // A video that goes out as a GIF has no cover.
        let animation = attachment.sends_as_animation(
            self.pending_attachments().len(),
            self.composer_group_media_effective(),
        );
        let cover_ok = !animation
            && self.session().is_some_and(|session| {
                session
                    .open_chat
                    .is_some_and(|chat| session_allows_cover(session, chat))
            });
        if cover_ok {
            let owner = cx.entity().downgrade();
            let has_cover = attachment.extras.cover.is_some();
            actions.push(
                Button::new(("composer-attach-cover", index as u64))
                    .icon(gpui_kit::assets::IconName::Image)
                    .xsmall()
                    .ghost()
                    .selected(has_cover)
                    .tooltip(tr(keys::Q_OPS_COVER))
                    .dropdown_menu(move |mut menu, _, _| {
                        let edit = owner.clone();
                        menu = menu.item(PopupMenuItem::new(tr(keys::Q_OPS_EDIT_COVER)).on_click(
                            move |_, _, cx| {
                                let _ = edit
                                    .update(cx, |this, cx| this.pick_attachment_cover(index, cx));
                            },
                        ));
                        if has_cover {
                            let clear = owner.clone();
                            menu = menu.item(
                                PopupMenuItem::new(tr(keys::Q_OPS_CLEAR_COVER)).on_click(
                                    move |_, _, cx| {
                                        let _ = clear.update(cx, |this, cx| {
                                            this.clear_attachment_cover(index, cx)
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    })
                    .into_any_element(),
            );
        }
        actions
    }

    /// "Edit Cover" on a tray video: choose an image (tdesktop
    /// `VideoCoverUploader::choose`). The picker answers later: the cover
    /// goes to this video in this chat, wherever the user is by then.
    fn pick_attachment_cover(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.tray_item(index) else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_OPS_CHOOSE_VIDEO_COVER).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first().cloned()
            {
                let _ = this.update(cx, |this, cx| this.set_attachment_cover(&item, &path, cx));
            }
        })
        .detach();
    }

    /// Stage a picked cover on the tray video `item`; nothing happens when
    /// that video has left its tray (sent, removed or replaced) meanwhile.
    pub(super) fn set_attachment_cover(
        &mut self,
        item: &TrayItem,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let Some(cover) = cover_send_path(path) else {
            self.connection
                .set_note(StatusNote::silent(quill::composer::EDIT_MEDIA_INVALID_FILE));
            cx.notify();
            return;
        };
        let name = cover
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image")
            .to_string();
        if self
            .composer_sends
            .update_item(item, |attachment| attachment.extras.cover = Some(cover))
            .is_none()
        {
            return;
        }
        self.connection.set_note(StatusNote::silent(tr_with(
            keys::Q_OPS_COVER_WILL_BE_SENT_WITH,
            &[("unwrap_or", &name)],
        )));
        cx.notify();
    }

    fn clear_attachment_cover(&mut self, index: usize, cx: &mut Context<Self>) {
        if self
            .update_pending_attachment(index, |attachment| attachment.extras.cover = None)
            .is_some()
        {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_COVER_CLEARED)));
            cx.notify();
        }
    }

    // The "Paid Content" box.

    pub(super) fn open_price_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slot) = self.send_slot() else {
            return;
        };
        let stars = self.composer_sends.get(&slot).paid_stars;
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_STARS_TO_UNLOCK))
                .submit_on_enter(false)
        });
        if stars > 0 {
            input.update(cx, |input, cx| {
                input.set_value(stars.to_string(), window, cx)
            });
        }
        self.composer_ui.send_box.price = Some(PriceDialog {
            input,
            too_high: false,
            slot,
        });
        cx.notify();
    }

    fn close_price_dialog(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.send_box.price = None;
        cx.notify();
    }

    /// Save the typed price; keeps the box open with an error when the
    /// price is over the limit.
    pub(super) fn submit_price_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.composer_ui.send_box.price.as_mut() else {
            return true;
        };
        let text = dialog.input.read(cx).value().to_string();
        let max = self.paid_media_star_max();
        let Some(dialog) = self.composer_ui.send_box.price.as_mut() else {
            return true;
        };
        match parse_paid_price(&text, max) {
            PriceInput::TooHigh => {
                dialog.too_high = true;
                cx.notify();
                false
            }
            input => {
                let price = match input {
                    PriceInput::Price(price) => price,
                    _ => 0,
                };
                let slot = dialog.slot.clone();
                self.composer_ui.send_box.price = None;
                self.composer_sends
                    .update(&slot, |send| send.paid_stars = price);
                self.connection.set_note(if price > 0 {
                    StatusNote::silent(tr_with(
                        keys::Q_OPS_THE_MEDIA_WILL_UNLOCK_FOR,
                        &[("price", &price.to_string())],
                    ))
                } else {
                    StatusNote::silent(tr(keys::Q_OPS_PAID_MEDIA_OFF))
                });
                cx.notify();
                true
            }
        }
    }

    pub(super) fn build_price_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PaidPrice, |this, _, cx| {
                this.close_price_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.composer_ui.send_box.price.as_ref() else {
                return dialog.on_close(on_close);
            };
            let muted = cx.theme().muted_foreground;
            let danger = cx.theme().danger;
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(tr(keys::Q_OPS_ENTER_UNLOCK_COST)),
                )
                .child(Input::new(&state.input).aria_label(tr(keys::Q_OPS_STARS_TO_UNLOCK_2)))
                .when(state.too_high, |body| {
                    body.child(div().text_sm().text_color(danger).child(tr_with(
                        keys::Q_LEFTOVERS_PRICE_UP_TO_STARS,
                        &[("count", &(this.paid_media_star_max()).to_string())],
                    )))
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_OPS_PEOPLE_WILL_HAVE_TO_TRANSFER)),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("paid-price-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_price_dialog(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::PaidPrice,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("paid-price-save")
                                .label(tr(keys::SETTINGS_SAVE))
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.submit_price_dialog(cx) {
                                        this.close_kit_dialog_if_done(
                                            DialogKind::PaidPrice,
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_PAID_CONTENT)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    PaidPrice => DialogSpec::new(
        5441,
        |app| app.composer_ui.send_box.price.is_some(),
        QuillApp::build_price_dialog,
    ),
}

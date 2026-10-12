//! The send box's media options (tdesktop `SendFilesBox`): "High quality"
//! for big photos, the price of paid media in channels (the "Paid Content"
//! box), a video's cover and "Edit Video" on its card.
//!
//! The choices live in `SendBoxUi` (`send_box_state.rs`) and ride the
//! snapshot as `MediaSendOptions`; covers and edits ride the attachment
//! (`AttachmentExtras`).

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
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
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// The "Paid Content" box (tdesktop `EditPriceBox`).
pub(super) struct PriceDialog {
    pub(super) input: Entity<InputState>,
    /// The typed price is over the limit (`field->showError()`).
    pub(super) too_high: bool,
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
                &self.composer_ui.pending_attachments,
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
            self.composer_ui.send_box.paid_stars
        } else {
            0
        }
    }

    /// Send-box choices for the snapshot of the next send.
    pub(super) fn composer_media_options(&self) -> MediaSendOptions {
        let as_files = self
            .composer_ui
            .pending_attachments
            .iter()
            .all(|attachment| attachment.kind == AttachmentKind::Document);
        MediaSendOptions {
            hd_photos: self.composer_ui.send_box.hd_photos && !as_files,
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
        self.composer_ui
            .pending_attachments
            .iter()
            .filter(|attachment| attachment.kind == AttachmentKind::Photo)
            .filter_map(|attachment| self.tray_photo_size(&attachment.path))
            .any(|(width, height)| can_use_high_quality(width, height))
    }

    /// Read the sound track of the new tray videos in the background; a
    /// video without one becomes a GIF when it lands (tdesktop
    /// `modifications.gif = !hasAudio`). Sending waits for the reads.
    pub(super) fn check_tray_videos(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<std::path::PathBuf> = self
            .composer_ui
            .pending_attachments
            .iter()
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
            cx.spawn(async move |this, cx| {
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
                            .composer_ui
                            .pending_attachments
                            .iter_mut()
                            .filter(|attachment| attachment.path == path)
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
    }

    /// Videos in the tray are still being read.
    pub(super) fn tray_videos_checking(&self) -> bool {
        !self.composer_ui.send_box.sound_checking.is_empty()
    }

    /// The options row of the tray: High quality and the price.
    pub(super) fn send_box_option_controls(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut controls = Vec::new();
        if self.tray_videos_checking() {
            controls.push(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Checking the video…")
                    .into_any_element(),
            );
        }
        if self.tray_has_large_photo() {
            controls.push(
                gpui_kit::component::checkbox::Checkbox::new("composer-high-quality")
                    .label("High quality")
                    .checked(self.composer_ui.send_box.hd_photos)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let send_box = &mut this.composer_ui.send_box;
                        send_box.hd_photos = !send_box.hd_photos;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if self.paid_media_available() {
            let stars = self.composer_ui.send_box.paid_stars;
            controls.push(
                Button::new("composer-paid-media")
                    .icon(gpui_kit::assets::IconName::Star)
                    .label(if stars > 0 {
                        format!("Unlock for {stars} Stars")
                    } else {
                        "Make paid".to_string()
                    })
                    .ghost()
                    .small()
                    .selected(stars > 0)
                    .tooltip(if stars > 0 {
                        "Change the price"
                    } else {
                        "Ask for Stars to see the media"
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
                .tooltip("Edit video")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_tray_video_editor(index, window, cx);
                }))
                .into_any_element(),
        ];
        // A video that goes out as a GIF has no cover.
        let animation = attachment.sends_as_animation(
            self.composer_ui.pending_attachments.len(),
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
                    .tooltip("Cover")
                    .dropdown_menu(move |mut menu, _, _| {
                        let edit = owner.clone();
                        menu = menu.item(PopupMenuItem::new("Edit Cover").on_click(
                            move |_, _, cx| {
                                let _ = edit
                                    .update(cx, |this, cx| this.pick_attachment_cover(index, cx));
                            },
                        ));
                        if has_cover {
                            let clear = owner.clone();
                            menu = menu.item(PopupMenuItem::new("Clear Cover").on_click(
                                move |_, _, cx| {
                                    let _ = clear.update(cx, |this, cx| {
                                        this.clear_attachment_cover(index, cx)
                                    });
                                },
                            ));
                        }
                        menu
                    })
                    .into_any_element(),
            );
        }
        actions
    }

    /// "Edit Cover" on a tray video: choose an image (tdesktop
    /// `VideoCoverUploader::choose`).
    fn pick_attachment_cover(&mut self, index: usize, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose video cover".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first().cloned()
            {
                let _ = this.update(cx, |this, cx| this.set_attachment_cover(index, &path, cx));
            }
        })
        .detach();
    }

    /// Stage a picked cover on a tray video.
    pub(super) fn set_attachment_cover(
        &mut self,
        index: usize,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let Some(attachment) = self.composer_ui.pending_attachments.get_mut(index) else {
            return;
        };
        match cover_send_path(path) {
            Some(cover) => {
                self.connection.status_note = format!(
                    "Cover {} will be sent with the video",
                    cover
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image")
                );
                attachment.extras.cover = Some(cover);
            }
            None => self.connection.status_note = quill::composer::EDIT_MEDIA_INVALID_FILE.into(),
        }
        cx.notify();
    }

    fn clear_attachment_cover(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(attachment) = self.composer_ui.pending_attachments.get_mut(index) {
            attachment.extras.cover = None;
            self.connection.status_note = "cover cleared".into();
            cx.notify();
        }
    }

    // The "Paid Content" box.

    pub(super) fn open_price_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stars = self.composer_ui.send_box.paid_stars;
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Stars to Unlock")
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
                self.composer_ui.send_box.price = None;
                self.composer_ui.send_box.paid_stars = price;
                self.connection.status_note = if price > 0 {
                    format!("The media will unlock for {price} Stars")
                } else {
                    "paid media off".into()
                };
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
                .child(div().text_sm().font_semibold().child("Enter Unlock Cost"))
                .child(Input::new(&state.input).aria_label("Stars to unlock"))
                .when(state.too_high, |body| {
                    body.child(div().text_sm().text_color(danger).child(format!(
                        "The price can be up to {} Stars.",
                        this.paid_media_star_max()
                    )))
                })
                .child(div().text_sm().text_color(muted).child(
                    "People will have to transfer this amount of Stars to your channel \
                     to view this media.",
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("paid-price-cancel")
                                .label("Cancel")
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
                                .label("Save")
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
                .title(crate::ui::shell::dialog_title("Paid Content"))
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

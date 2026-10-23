//! The wallpaper preview (tdesktop `boxes/background_preview_box.cpp`):
//! opened from the gallery, a `bg/` link or "From file…". It shows the
//! wallpaper behind two sample messages, offers Blur for photos and Share
//! (copy the `bg/` link, not for local files), and Apply sets it as the
//! account wallpaper for the current light or dark theme.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_look_ui::is_wallpaper_image;
use super::shell::{DialogKind, QuillShell};
use super::wallpaper::{Wallpaper, session_wallpaper};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::{Background, BackgroundType};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What is being previewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PreviewSource {
    /// An installed wallpaper, by background id.
    Installed(i64),
    /// A `bg/` link; the `searchBackground` answer arrives in the session.
    Link(String),
    /// A local image from "From file…".
    File(PathBuf),
}

/// The preview's pending choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WallpaperPreview {
    pub source: PreviewSource,
    /// The Blur box, once the user touched it (`None` keeps the
    /// background's own setting).
    pub blur: Option<bool>,
    /// A line under the preview ("Link copied").
    pub note: Option<String>,
}

/// The type Apply and Share send: the background's own, with Blur as
/// chosen for photos.
pub(super) fn chosen_type(kind: &BackgroundType, blur: Option<bool>) -> BackgroundType {
    match (kind, blur) {
        (BackgroundType::Wallpaper { moving, .. }, Some(blurred)) => BackgroundType::Wallpaper {
            blurred,
            moving: *moving,
        },
        _ => kind.clone(),
    }
}

/// Whether a background offers Blur (photos, not fills or patterns).
pub(super) fn offers_blur(kind: &BackgroundType) -> bool {
    matches!(kind, BackgroundType::Wallpaper { .. })
}

impl QuillApp {
    pub(super) fn open_wallpaper_preview(&mut self, source: PreviewSource, cx: &mut Context<Self>) {
        if let Some(session) = self.wallpaper_session_mut() {
            session.chats_state.background_error = None;
            session.chats_state.background_url = None;
        }
        self.dialogs.wallpaper_preview = Some(WallpaperPreview {
            source,
            blur: None,
            note: None,
        });
        cx.notify();
    }

    /// `bg/<name>` links: search the background and preview it.
    pub(super) fn open_background_link(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.chats_state.background_error = None;
            if live.driver.search_background(&name).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_OPEN_THE_WALLPAPER,
                )));
                return;
            }
        }
        self.open_wallpaper_preview(PreviewSource::Link(name), cx);
    }

    /// "From file…": pick an image and preview it.
    pub(super) fn choose_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MISC_CHOOSE_WALLPAPER).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.preview_wallpaper_file(path, cx));
            }
        })
        .detach();
    }

    fn preview_wallpaper_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !is_wallpaper_image(&path) {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_CHOOSE_A_JPEG_PNG_OR)));
            cx.notify();
            return;
        }
        self.open_wallpaper_preview(PreviewSource::File(path), cx);
    }

    fn wallpaper_session_mut(&mut self) -> Option<&mut quill::state::Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// The background behind an installed or link preview.
    fn previewed_background(&self, source: &PreviewSource) -> Option<Background> {
        let session = self.session()?;
        match source {
            PreviewSource::Installed(id) => session
                .chats_state
                .installed_backgrounds
                .iter()
                .flatten()
                .chain(session.chats_state.default_backgrounds.values())
                .find(|b| b.id == *id)
                .cloned(),
            PreviewSource::Link(_) => session.chats_state.searched_background.clone(),
            PreviewSource::File(_) => None,
        }
    }

    pub(super) fn close_wallpaper_preview(&mut self, cx: &mut Context<Self>) {
        self.dialogs.wallpaper_preview = None;
        if let Some(session) = self.wallpaper_session_mut() {
            session.chats_state.searched_background = None;
            session.chats_state.background_url = None;
        }
        cx.notify();
    }

    fn apply_wallpaper_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.dialogs.wallpaper_preview.clone() else {
            return;
        };
        let dark = cx.theme().is_dark();
        let background = self.previewed_background(&preview.source);
        match (&preview.source, self.live.as_mut()) {
            (PreviewSource::File(path), Some(live)) => {
                let blurred = preview.blur.unwrap_or(false);
                if let Err(err) =
                    live.driver
                        .set_default_background_local(&path.to_string_lossy(), blurred, dark)
                {
                    self.connection.set_note(StatusNote::error(user_error_for(
                        keys::Q_NOTES_FAILED_SET_WALLPAPER,
                        &err,
                    )));
                    return;
                }
            }
            (PreviewSource::File(_), None) => {}
            (_, live) => {
                let Some(background) = background else {
                    return;
                };
                let kind = chosen_type(&background.kind, preview.blur);
                match live {
                    Some(live) => {
                        if let Err(err) =
                            live.driver
                                .set_default_background(background.id, Some(&kind), dark)
                        {
                            self.connection.set_note(StatusNote::error(user_error_for(
                                keys::Q_NOTES_FAILED_SET_WALLPAPER,
                                &err,
                            )));
                            return;
                        }
                    }
                    None => {
                        // Screenshot demo: apply locally.
                        if let Some(session) = self.demo_session.as_mut() {
                            session
                                .chats_state
                                .default_backgrounds
                                .insert(dark, Background { kind, ..background });
                        }
                    }
                }
            }
        }
        self.set_appearance(cx, |a| {
            a.telegram_wallpaper = true;
            a.wallpaper_rgb = None;
        });
        self.close_wallpaper_preview(cx);
    }

    /// Share: ask TDLib for the `bg/` link; [`Self::drain_wallpaper_preview`]
    /// copies it.
    fn share_wallpaper_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.dialogs.wallpaper_preview.clone() else {
            return;
        };
        let Some(background) = self.previewed_background(&preview.source) else {
            return;
        };
        let kind = chosen_type(&background.kind, preview.blur);
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .get_background_url(&background.name, &kind)
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MISC_COULD_NOT_MAKE_THE_WALLPAPER,
            )));
        }
        cx.notify();
    }

    /// Poll-loop half of Share: copy the link once it arrives.
    pub(super) fn drain_wallpaper_preview(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(url) = self
            .wallpaper_session_mut()
            .and_then(|s| s.chats_state.background_url.take())
        else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        if let Some(preview) = self.dialogs.wallpaper_preview.as_mut() {
            preview.note = Some(tr(keys::CHANNEL_PUBLIC_LINK_COPIED).into());
        }
        cx.notify();
        true
    }

    /// What the preview paints, with the Blur choice applied.
    fn preview_paint(&self, preview: &WallpaperPreview) -> Option<Wallpaper> {
        let wallpaper = match &preview.source {
            PreviewSource::File(path) => Some(Wallpaper::Image {
                path: path.clone(),
                blurred: false,
                tiled: false,
            }),
            source => {
                let background = self.previewed_background(source)?;
                self.session()
                    .and_then(|s| session_wallpaper(s, &background))
            }
        }?;
        Some(match preview.blur {
            Some(blur) => wallpaper.with_blur(blur),
            None => wallpaper,
        })
    }

    pub(super) fn build_wallpaper_preview_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::WallpaperPreview, |this, _, cx| {
                this.close_wallpaper_preview(cx);
            });
        app.update(cx, |this, cx| {
            let Some(preview) = this.dialogs.wallpaper_preview.clone() else {
                return dialog
                    .overlay(true)
                    .title(super::shell::dialog_title(tr(keys::Q_MISC_WALLPAPER)))
                    .on_close(on_close);
            };
            let muted = cx.theme().muted_foreground;
            let background = this.previewed_background(&preview.source);
            // The gallery only fetched a thumbnail: the preview needs the
            // full file (deduped while it downloads).
            if let (Some(bg), Some(live)) = (&background, this.live.as_mut()) {
                live.driver.download_background_file(bg);
            }
            let error = this
                .session()
                .and_then(|s| s.chats_state.background_error.clone());
            let is_file = matches!(preview.source, PreviewSource::File(_));
            let ready = is_file || background.is_some();
            let mut body = div().flex().flex_col().gap_3();
            match (&preview.source, &background, &error) {
                (PreviewSource::Link(name), None, None) => {
                    body = body.child(div().text_xs().text_color(muted).child(tr_with(
                        keys::Q_MISC_LOOKING_UP,
                        &[("name", &name.to_string())],
                    )));
                }
                (PreviewSource::Link(_), None, Some(error)) => {
                    body = body.child(
                        div()
                            .id("wallpaper-preview-link-error")
                            .text_sm()
                            .text_color(super::danger_dark())
                            .child(tr_with(
                                keys::Q_LEFTOVERS_WALLPAPER_LINK_INVALID,
                                &[("error", &error.to_string())],
                            )),
                    );
                }
                _ => {
                    let paint = this.preview_paint(&preview);
                    body = body.child(this.chat_look_preview(paint.as_ref(), 0, None, cx));
                    if let PreviewSource::File(path) = &preview.source {
                        body = body.child(div().text_xs().text_color(muted).child(tr_with(
                            keys::Q_MISC_FROM_FILE_2,
                            &[("file_label", &(file_label(path)).to_string())],
                        )));
                    }
                }
            }
            let blur_offered = is_file || background.as_ref().is_some_and(|b| offers_blur(&b.kind));
            if blur_offered && ready {
                let checked = preview.blur.unwrap_or_else(|| {
                    matches!(
                        background.as_ref().map(|b| &b.kind),
                        Some(BackgroundType::Wallpaper { blurred: true, .. })
                    )
                });
                body = body.child(
                    Checkbox::new("wallpaper-preview-blur")
                        .checked(checked)
                        .label(tr(keys::Q_MISC_BLUR))
                        .on_click(cx.listener(|this, &on, _, cx| {
                            if let Some(p) = this.dialogs.wallpaper_preview.as_mut() {
                                p.blur = Some(on);
                            }
                            cx.notify();
                        })),
                );
            }
            if ready {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_SETS_THIS_WALLPAPER_FOR_YOUR)),
                );
            }
            if let Some(note) = &preview.note {
                body = body.child(div().text_xs().text_color(muted).child(note.clone()));
            }
            if let (Some(error), true) = (&error, ready) {
                body = body.child(
                    div()
                        .id("wallpaper-preview-error")
                        .text_sm()
                        .text_color(super::danger_dark())
                        .child(tr_with(
                            keys::Q_MISC_COULDN_T_UPDATE_THE_WALLPAPER,
                            &[("error", &error.to_string())],
                        )),
                );
            }
            let can_share = background.is_some() && this.live.is_some();
            let footer = div()
                .flex()
                .items_center()
                .gap_2()
                .when(can_share, |row| {
                    row.child(
                        Button::new("wallpaper-preview-share")
                            .label(tr(keys::Q_MISC_SHARE))
                            .ghost()
                            .tooltip(tr(keys::Q_MISC_COPY_A_LINK_TO_THIS))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.share_wallpaper_preview(cx)),
                            ),
                    )
                })
                .child(div().flex_1())
                .child(
                    Button::new("wallpaper-preview-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_wallpaper_preview(cx);
                            this.close_kit_dialog_if_done(DialogKind::WallpaperPreview, window, cx);
                        })),
                )
                .child(
                    Button::new("wallpaper-preview-apply")
                        .label(tr(keys::Q_MISC_APPLY))
                        .primary()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_wallpaper_preview(cx);
                            this.close_kit_dialog_if_done(DialogKind::WallpaperPreview, window, cx);
                        })),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body.into_any_element())));
            dialog
                .overlay(true)
                .width(px(440.))
                .title(super::shell::dialog_title(tr(keys::Q_MISC_WALLPAPER)))
                .content(super::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

/// The file name shown for a local wallpaper.
pub(super) fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

crate::ui::shell::register_dialogs! {
    WallpaperPreview => DialogSpec::new(
        2110,
        |app| app.dialogs.wallpaper_preview.is_some(),
        QuillApp::build_wallpaper_preview_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{chosen_type, file_label, offers_blur};
    use quill::telegram::envelope::{BackgroundFill, BackgroundType};
    use std::path::Path;

    #[test]
    fn blur_is_offered_for_photos_and_sent_with_their_type() {
        let photo = BackgroundType::Wallpaper {
            blurred: false,
            moving: true,
        };
        assert!(offers_blur(&photo));
        assert_eq!(
            chosen_type(&photo, Some(true)),
            BackgroundType::Wallpaper {
                blurred: true,
                moving: true
            }
        );
        assert_eq!(chosen_type(&photo, None), photo, "untouched keeps its own");
        let pattern = BackgroundType::Pattern {
            fill: BackgroundFill::Solid(1),
            intensity: 30,
            inverted: false,
            moving: false,
        };
        assert!(!offers_blur(&pattern));
        assert_eq!(chosen_type(&pattern, Some(true)), pattern);
        assert!(!offers_blur(&BackgroundType::Fill(BackgroundFill::Solid(
            1
        ))));
    }

    #[test]
    fn local_files_show_their_name() {
        assert_eq!(file_label(Path::new("/a/b/sunset.jpg")), "sunset.jpg");
    }
}

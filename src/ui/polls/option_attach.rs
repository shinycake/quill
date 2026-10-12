//! Attaching media to the options of a new poll, and to the option added to
//! an open poll, as tdesktop's create box offers it (`create_poll_box.cpp`,
//! `showMediaMenu`; `history_view_add_poll_option.cpp`): a photo or video
//! from a file, a sticker from the sticker panel, a link, and removing it
//! again. tdesktop also offers a place when its map picker is available;
//! Quill has no map picker, so that entry is not shown (tdesktop hides it
//! too when the picker is unavailable).

use super::super::app::QuillApp;
use super::super::dialogs::{OptionAttach, OptionMediaSlot};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{AttachmentKind, media_kind_for};
use quill::poll::PollOptionInput;
use quill::state::StickerTab;
use std::path::PathBuf;

/// TDLib: an `inputPhoto` photo is at most 10 MB.
const PHOTO_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Whose media it is: a row of the new poll, or the option being added to a
/// poll that is already sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum AttachTarget {
    Row(usize),
    Added,
}

impl AttachTarget {
    fn key(self) -> String {
        match self {
            Self::Row(index) => index.to_string(),
            Self::Added => "add".into(),
        }
    }
}

impl QuillApp {
    fn attach_ref(&self, target: AttachTarget) -> Option<&OptionAttach> {
        match target {
            AttachTarget::Row(index) => self.composer_ui.poll_dialog.as_ref()?.attach.get(index),
            AttachTarget::Added => self
                .message_ui
                .poll_add_option
                .as_ref()
                .map(|panel| &panel.attach),
        }
    }

    fn attach_mut(&mut self, target: AttachTarget) -> Option<&mut OptionAttach> {
        match target {
            AttachTarget::Row(index) => {
                self.composer_ui.poll_dialog.as_mut()?.attach.get_mut(index)
            }
            AttachTarget::Added => self
                .message_ui
                .poll_add_option
                .as_mut()
                .map(|panel| &mut panel.attach),
        }
    }

    /// Every media slot being written, rows first.
    fn attach_slots(&mut self) -> Vec<&mut OptionAttach> {
        let mut slots: Vec<&mut OptionAttach> = Vec::new();
        if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
            slots.extend(dialog.attach.iter_mut());
        }
        if let Some(panel) = self.message_ui.poll_add_option.as_mut() {
            slots.push(&mut panel.attach);
        }
        slots
    }

    /// The paperclip at the end of an option's field.
    pub(in crate::ui) fn poll_option_attach_button(
        &self,
        target: AttachTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(format!("poll-attach-option-{}", target.key()))
            .icon(IconName::Image)
            .ghost()
            .tooltip("Attach a picture, sticker or link")
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(attach) = this.attach_mut(target) {
                    attach.menu_open = !attach.menu_open;
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// What sits under an option's field: the menu, the link field, the
    /// choice of a sticker, or the attached media.
    pub(in crate::ui) fn poll_option_attach_area(
        &self,
        target: AttachTarget,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let attach = self.attach_ref(target)?;
        let key = target.key();
        let muted = cx.theme().muted_foreground;
        let mut area = div()
            .id(SharedString::from(format!("poll-option-attach-{key}")))
            .flex()
            .items_center()
            .gap_2()
            .px_2();
        let mut shown = false;
        if let Some(input) = &attach.link_input {
            shown = true;
            area = area
                .child(
                    div().flex_1().child(
                        Textarea::new(input)
                            .aria_label("Link for this option")
                            .h(px(36.)),
                    ),
                )
                .child(
                    Button::new(format!("poll-option-link-done-{key}"))
                        .label("Done")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.finish_poll_option_link(target, cx)
                        })),
                )
                .child(
                    Button::new(format!("poll-option-link-cancel-{key}"))
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(attach) = this.attach_mut(target) {
                                attach.link_input = None;
                            }
                            cx.notify();
                        })),
                );
        } else if attach.sticker_wait {
            shown = true;
            area = area
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Choose a sticker in the sticker panel"),
                )
                .child(
                    Button::new(format!("poll-option-sticker-cancel-{key}"))
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(attach) = this.attach_mut(target) {
                                attach.sticker_wait = false;
                            }
                            cx.notify();
                        })),
                );
        } else if attach.menu_open {
            shown = true;
            let entry = |id: &'static str, label: &'static str| {
                Button::new(format!("poll-option-{id}-{key}"))
                    .label(label)
                    .ghost()
            };
            area = area
                .child(entry("photo-video", "Photo or video").on_click(
                    cx.listener(move |this, _, _, cx| this.pick_poll_option_file(target, cx)),
                ))
                .child(entry("sticker", "Sticker").on_click(
                    cx.listener(move |this, _, _, cx| this.choose_poll_option_sticker(target, cx)),
                ))
                .child(
                    entry("link", "Link").on_click(cx.listener(move |this, _, window, cx| {
                        this.start_poll_option_link(target, window, cx)
                    })),
                );
        }
        if attach.media != OptionMediaSlot::None {
            shown = true;
            let (label, thumb): (String, Option<PathBuf>) = match &attach.media {
                OptionMediaSlot::Preparing(path) => {
                    (format!("Preparing {}…", file_name(path)), None)
                }
                OptionMediaSlot::Ready(PollOptionInput::Photo { path }) => (
                    format!("Photo {}", file_name(path)),
                    Some(PathBuf::from(path)),
                ),
                OptionMediaSlot::Ready(PollOptionInput::Video { path, duration, .. }) => (
                    format!(
                        "Video {} ({})",
                        file_name(path),
                        quill::voice::format_voice_duration(*duration)
                    ),
                    None,
                ),
                OptionMediaSlot::Ready(PollOptionInput::Sticker { emoji, .. }) => {
                    (format!("Sticker {emoji}"), None)
                }
                OptionMediaSlot::Ready(PollOptionInput::Link { url }) => {
                    (format!("Link {url}"), None)
                }
                OptionMediaSlot::None => (String::new(), None),
            };
            area = area
                .children(thumb.map(|path| {
                    img(path)
                        .size(px(32.))
                        .rounded_md()
                        .object_fit(ObjectFit::Cover)
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(label),
                )
                .child(
                    Button::new(format!("poll-option-media-remove-{key}"))
                        .label("Remove")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.clear_poll_option_media(target, cx)
                        })),
                );
        }
        shown.then(|| area.into_any_element())
    }

    fn set_poll_option_media(&mut self, target: AttachTarget, slot: OptionMediaSlot) {
        if let Some(attach) = self.attach_mut(target) {
            attach.media = slot;
            attach.menu_open = false;
        }
    }

    pub(super) fn clear_poll_option_media(&mut self, target: AttachTarget, cx: &mut Context<Self>) {
        self.set_poll_option_media(target, OptionMediaSlot::None);
        cx.notify();
    }

    /// "Photo or video": pick a file, then read it off the UI thread.
    pub(super) fn pick_poll_option_file(&mut self, target: AttachTarget, cx: &mut Context<Self>) {
        if let Some(attach) = self.attach_mut(target) {
            attach.menu_open = false;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.take_poll_option_file(target, path, cx));
        })
        .detach();
    }

    /// A file was picked for `target`.
    fn take_poll_option_file(
        &mut self,
        target: AttachTarget,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = media_kind_for(&path) else {
            self.connection.status_note = "choose a photo or a video".into();
            cx.notify();
            return;
        };
        let shown = path.to_string_lossy().into_owned();
        self.set_poll_option_media(target, OptionMediaSlot::Preparing(shown.clone()));
        cx.notify();
        let read = cx.background_executor().spawn({
            let path = path.clone();
            async move {
                match kind {
                    AttachmentKind::Video => {
                        quill::video::probe_local_video(&path).map(|probe| PollOptionInput::Video {
                            path: path.to_string_lossy().into_owned(),
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                        })
                    }
                    _ => match std::fs::metadata(&path) {
                        Ok(meta) if meta.len() > PHOTO_MAX_BYTES => {
                            Err("the photo is over 10 MB".to_string())
                        }
                        Ok(_) => Ok(PollOptionInput::Photo {
                            path: path.to_string_lossy().into_owned(),
                        }),
                        Err(_) => Err("could not read the file".to_string()),
                    },
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_poll_option_file(&shown, result, cx)
            });
        })
        .detach();
    }

    /// The file for the option preparing `shown` was read.
    fn finish_poll_option_file(
        &mut self,
        shown: &str,
        result: Result<PollOptionInput, String>,
        cx: &mut Context<Self>,
    ) {
        let waiting = OptionMediaSlot::Preparing(shown.to_string());
        let mut note = None;
        for attach in self.attach_slots() {
            if attach.media != waiting {
                continue;
            }
            match &result {
                Ok(media) => attach.media = OptionMediaSlot::Ready(media.clone()),
                Err(why) => {
                    attach.media = OptionMediaSlot::None;
                    note = Some(why.clone());
                }
            }
            break;
        }
        // Nothing waiting: the option was removed or its media replaced.
        if let Some(note) = note {
            self.connection.status_note = note;
        }
        cx.notify();
    }

    /// "Sticker": open the sticker panel; the next sticker clicked in it goes
    /// to this option instead of being sent.
    pub(super) fn choose_poll_option_sticker(
        &mut self,
        target: AttachTarget,
        cx: &mut Context<Self>,
    ) {
        for attach in self.attach_slots() {
            attach.sticker_wait = false;
        }
        if let Some(attach) = self.attach_mut(target) {
            attach.menu_open = false;
            attach.link_input = None;
            attach.sticker_wait = true;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_sticker_panel();
            let _ = live.driver.select_sticker_tab(StickerTab::Installed);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.open = true;
            session.stickers.stickers.tab = StickerTab::Installed;
        }
        cx.notify();
    }

    /// A sticker clicked in the panel while it chooses a poll option's
    /// sticker. `true` when it was taken here (it is not sent).
    pub(in crate::ui) fn route_poll_option_sticker(
        &mut self,
        file_id: quill::ids::FileId,
        emoji: &str,
        size: (i32, i32),
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.sticker_panel_open() {
            return false;
        }
        let rows = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .map_or(0, |dialog| dialog.attach.len());
        let target = (0..rows)
            .map(AttachTarget::Row)
            .chain([AttachTarget::Added])
            .find(|target| self.attach_ref(*target).is_some_and(|a| a.sticker_wait));
        let Some(target) = target else {
            return false;
        };
        self.set_poll_option_media(
            target,
            OptionMediaSlot::Ready(PollOptionInput::Sticker {
                file_id: file_id.0,
                width: size.0,
                height: size.1,
                emoji: emoji.to_string(),
            }),
        );
        if let Some(attach) = self.attach_mut(target) {
            attach.sticker_wait = false;
        }
        self.close_sticker_panel(cx);
        true
    }

    /// "Link": a field for the address (tdesktop `AddPollOptionLinkBox`).
    pub(super) fn start_poll_option_link(
        &mut self,
        target: AttachTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .attach_ref(target)
            .and_then(|attach| match &attach.media {
                OptionMediaSlot::Ready(PollOptionInput::Link { url }) => Some(url.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("URL")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| {
            input.set_value(current, window, cx);
            input.focus(window, cx);
        });
        if let Some(attach) = self.attach_mut(target) {
            attach.menu_open = false;
            attach.sticker_wait = false;
            attach.link_input = Some(input);
        }
        cx.notify();
    }

    /// "Done" on the link field: an empty address is refused, as tdesktop's
    /// box marks its field.
    pub(super) fn finish_poll_option_link(&mut self, target: AttachTarget, cx: &mut Context<Self>) {
        let Some(url) = self
            .attach_ref(target)
            .and_then(|attach| attach.link_input.as_ref())
            .map(|input| input.read(cx).value().trim().to_string())
        else {
            return;
        };
        if url.is_empty() {
            self.connection.status_note = "enter the link's address".into();
            cx.notify();
            return;
        }
        self.set_poll_option_media(
            target,
            OptionMediaSlot::Ready(PollOptionInput::Link { url }),
        );
        if let Some(attach) = self.attach_mut(target) {
            attach.link_input = None;
        }
        cx.notify();
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(all(test, feature = "demo-capture"))]
#[path = "option_attach_tests.rs"]
mod tests;

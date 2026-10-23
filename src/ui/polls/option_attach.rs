//! Attaching media to the options of a new poll, and to the option added to
//! an open poll, as tdesktop's create box offers it (`create_poll_box.cpp`,
//! `showMediaMenu`; `history_view_add_poll_option.cpp`): a photo or video
//! from a file, a sticker from the sticker panel, a link, and removing it
//! again. tdesktop also offers a place when its map picker is available;
//! Quill has no map picker, so that entry is not shown (tdesktop hides it
//! too when the picker is unavailable).

use super::super::app::QuillApp;
use super::super::composer_field::emoji_token;
use super::super::dialogs::{OptionAttach, OptionMediaSlot};
use super::super::media_panel::PanelTab;
use crate::ui::status_note::StatusNote;
use gpui_base::input::InputContent;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{AttachmentKind, media_kind_for};
use quill::lang::{keys, tr, tr_with};
use quill::poll::PollOptionInput;
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

    /// The field of an option being written.
    fn option_input(&self, target: AttachTarget) -> Option<Entity<TextareaState>> {
        match target {
            AttachTarget::Row(index) => self
                .composer_ui
                .poll_dialog
                .as_ref()?
                .option_inputs
                .get(index)
                .cloned(),
            AttachTarget::Added => self
                .message_ui
                .poll_add_option
                .as_ref()
                .map(|panel| panel.input.clone()),
        }
    }

    /// The smiley at the end of an option's field: the emoji panel opens for
    /// it (custom emoji need Premium, as in tdesktop).
    pub(in crate::ui) fn poll_option_emoji_button(
        &self,
        target: AttachTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(format!("poll-emoji-option-{}", target.key()))
            .icon(IconName::FaceSlightlySmiling)
            .ghost()
            .tooltip(tr(keys::Q_MEDIA_EMOJI))
            .on_click(cx.listener(move |this, _, _, cx| this.choose_poll_option_emoji(target, cx)))
            .into_any_element()
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
            .tooltip(tr(keys::Q_MEDIA_ATTACH_A_PICTURE_STICKER_OR))
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
                            .aria_label(tr(keys::Q_MEDIA_LINK_FOR_THIS_OPTION))
                            .h(px(36.)),
                    ),
                )
                .child(
                    Button::new(format!("poll-option-link-done-{key}"))
                        .label(tr(keys::BOX_DONE))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.finish_poll_option_link(target, cx)
                        })),
                )
                .child(
                    Button::new(format!("poll-option-link-cancel-{key}"))
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(attach) = this.attach_mut(target) {
                                attach.link_input = None;
                            }
                            cx.notify();
                        })),
                );
        } else if attach.emoji_wait {
            shown = true;
            area = area
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MEDIA_CHOOSE_EMOJI_IN_THE_EMOJI)),
                )
                .child(
                    Button::new(format!("poll-option-emoji-done-{key}"))
                        .label(tr(keys::BOX_DONE))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(attach) = this.attach_mut(target) {
                                attach.emoji_wait = false;
                            }
                            this.close_media_panel(cx);
                        })),
                );
        } else if attach.sticker_wait {
            shown = true;
            area = area
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MEDIA_CHOOSE_A_STICKER_IN_THE)),
                )
                .child(
                    Button::new(format!("poll-option-sticker-cancel-{key}"))
                        .label(tr(keys::CANCEL))
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
                .child(
                    entry("photo-video", tr(keys::ATTACH_PHOTO_OR_VIDEO)).on_click(
                        cx.listener(move |this, _, _, cx| this.pick_poll_option_file(target, cx)),
                    ),
                )
                .child(entry("sticker", tr(keys::Q_MEDIA_STICKER)).on_click(
                    cx.listener(move |this, _, _, cx| this.choose_poll_option_sticker(target, cx)),
                ))
                .child(
                    entry("link", tr(keys::POLLS_CREATE_OPTION_LINK)).on_click(cx.listener(
                        move |this, _, window, cx| this.start_poll_option_link(target, window, cx),
                    )),
                );
        }
        if attach.media != OptionMediaSlot::None {
            shown = true;
            let (label, thumb): (String, Option<PathBuf>) = match &attach.media {
                OptionMediaSlot::Preparing(path) => (
                    tr_with(
                        keys::Q_MEDIA_PREPARING,
                        &[("file_name", &(file_name(path)).to_string())],
                    ),
                    None,
                ),
                OptionMediaSlot::Ready(PollOptionInput::Photo { path }) => (
                    tr_with(
                        keys::Q_MEDIA_PHOTO,
                        &[("file_name", &(file_name(path)).to_string())],
                    ),
                    Some(PathBuf::from(path)),
                ),
                OptionMediaSlot::Ready(PollOptionInput::Video { path, duration, .. }) => (
                    tr_with(
                        keys::Q_LEFTOVERS_VIDEO_NAME_SIZE,
                        &[
                            ("name", &(file_name(path)).to_string()),
                            (
                                "size",
                                &(quill::voice::format_voice_duration(*duration)).to_string(),
                            ),
                        ],
                    ),
                    attach.preview.clone(),
                ),
                OptionMediaSlot::Ready(PollOptionInput::Sticker { emoji, .. }) => (
                    tr_with(keys::Q_MEDIA_STICKER_2, &[("emoji", &emoji.to_string())]),
                    None,
                ),
                OptionMediaSlot::Ready(PollOptionInput::Link { url }) => (
                    tr_with(keys::Q_MEDIA_LINK, &[("url", &url.to_string())]),
                    None,
                ),
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
                        .label(tr(keys::BOX_REMOVE))
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
            attach.preview = None;
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
            prompt: Some(tr(keys::Q_MEDIA_ATTACH).into()),
        });
        self.spawn_scoped(cx, async move |this, cx| {
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_CHOOSE_A_PHOTO_OR_A)));
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
                    AttachmentKind::Video => quill::video::probe_local_video(&path).map(|probe| {
                        // A frame to show while the clip is attached; the
                        // clip still goes without one if ffmpeg cannot.
                        let frame = quill::video_edit::render_frame(&path, 0, 0, false).ok();
                        (
                            PollOptionInput::Video {
                                path: path.to_string_lossy().into_owned(),
                                duration: probe.duration,
                                width: probe.width,
                                height: probe.height,
                            },
                            frame,
                        )
                    }),
                    _ => match std::fs::metadata(&path) {
                        Ok(meta) if meta.len() > PHOTO_MAX_BYTES => {
                            Err(tr(keys::Q_MEDIA_THE_PHOTO_IS_OVER_10).to_string())
                        }
                        Ok(_) => Ok((
                            PollOptionInput::Photo {
                                path: path.to_string_lossy().into_owned(),
                            },
                            None,
                        )),
                        Err(_) => Err(tr(keys::Q_MEDIA_COULD_NOT_READ_THE_FILE).to_string()),
                    },
                }
            }
        });
        self.spawn_scoped(cx, async move |this, cx| {
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
        result: Result<(PollOptionInput, Option<PathBuf>), String>,
        cx: &mut Context<Self>,
    ) {
        let waiting = OptionMediaSlot::Preparing(shown.to_string());
        let mut note = None;
        for attach in self.attach_slots() {
            if attach.media != waiting {
                continue;
            }
            match &result {
                Ok((media, frame)) => {
                    attach.media = OptionMediaSlot::Ready(media.clone());
                    attach.preview = frame.clone();
                }
                Err(why) => {
                    attach.media = OptionMediaSlot::None;
                    note = Some(why.clone());
                }
            }
            break;
        }
        // Nothing waiting: the option was removed or its media replaced.
        if let Some(note) = note {
            self.connection.set_note(StatusNote::error(note));
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
        self.open_media_panel(PanelTab::Stickers, cx);
    }

    /// The smiley: the emoji panel opens, and the emoji picked in it go into
    /// this option's field instead of the composer.
    pub(super) fn choose_poll_option_emoji(
        &mut self,
        target: AttachTarget,
        cx: &mut Context<Self>,
    ) {
        for attach in self.attach_slots() {
            attach.emoji_wait = false;
        }
        if let Some(attach) = self.attach_mut(target) {
            attach.menu_open = false;
            attach.link_input = None;
            attach.sticker_wait = false;
            attach.emoji_wait = true;
        }
        self.open_media_panel(PanelTab::Emoji, cx);
    }

    /// The field an emoji pick goes to, while the panel is open for one.
    fn poll_emoji_field(&self) -> Option<Entity<TextareaState>> {
        if !self.pickers.media_panel.open {
            return None;
        }
        let rows = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .map_or(0, |dialog| dialog.attach.len());
        (0..rows)
            .map(AttachTarget::Row)
            .chain([AttachTarget::Added])
            .find(|target| self.attach_ref(*target).is_some_and(|a| a.emoji_wait))
            .and_then(|target| self.option_input(target))
    }

    /// An emoji picked in the panel. `true` when it went to an option.
    pub(in crate::ui) fn route_poll_option_emoji(
        &mut self,
        emoji: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(field) = self.poll_emoji_field() else {
            return false;
        };
        field.update(cx, |input, cx| {
            input.replace(emoji, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
        true
    }

    /// A custom emoji picked in the panel: an inline token in the field, as
    /// in the composer. `true` when it went to an option.
    pub(in crate::ui) fn route_poll_option_custom_emoji(
        &mut self,
        fallback: &str,
        custom_emoji_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(field) = self.poll_emoji_field() else {
            return false;
        };
        let text = if fallback.is_empty() {
            "\u{2B50}"
        } else {
            fallback
        }
        .to_string();
        let len = text.len();
        let content = InputContent::new(text.clone())
            .with_token(0..len, emoji_token(custom_emoji_id, &text))
            .unwrap_or_else(|_| InputContent::new(text));
        field.update(cx, |input, cx| {
            let range = input.selected_range();
            if input
                .replace_range_with_content(range, content.clone(), window, cx)
                .is_err()
            {
                input.replace(content.text().to_string(), window, cx);
            }
            input.focus(window, cx);
        });
        cx.notify();
        true
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
        if !self.pickers.media_panel.open {
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
        cx.notify();
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
                .placeholder(tr(keys::Q_POLLS_LINK_URL))
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
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MEDIA_ENTER_THE_LINKS_ADDRESS)));
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "option_attach_tests.rs"]
mod tests;

//! The macOS Touch Bar, after Telegram Desktop's `platform/mac/touchbar/`
//! (see [`model`] for what it shows and when).
//!
//! tdesktop builds it only where there is a Touch Bar, and its experimental
//! option "Disable Touch Bar (macOS only)." switches it off
//! (`quill::advanced::experimental::touchbar_disabled`). Windows and Linux
//! have no Touch Bar, so this module is built for macOS only; everything it offers is in the
//! window on every platform.
//!
//! The app computes a [`model::Bar`] whenever it or the composer changes,
//! and the AppKit side mirrors it. Touches come back as [`model::Command`]s
//! through a channel and run here on the app.
#![deny(clippy::indexing_slicing)]

pub(crate) mod model;

mod mac;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

use super::app::QuillApp;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use model::{
    Bar, BarInputs, Command, FormatCommand, PickerItem, Pin, PinKind, PinnedPanel, StickerSection,
    Track, ViewerBar,
};
use quill::ids::ChatId;
use quill::media_viewer::MediaViewerKind;
use quill::send_rights::SendKind;
use quill::telegram::envelope::AuthorizationState;

/// Colors the AppKit side draws with, from the app's palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Palette {
    /// Saved Messages' disc.
    pub(crate) accent: Rgba,
    /// The archive's disc.
    pub(crate) archive: Rgba,
    /// Glyphs on those discs and badge digits.
    pub(crate) on_fill: Rgba,
    pub(crate) badge: Rgba,
    pub(crate) badge_muted: Rgba,
    pub(crate) online: Rgba,
}

/// The sticker and emoji scrubbers' contents.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PickerContents {
    pub(crate) stickers: Vec<PickerItem>,
    /// Shared: the list is long and rebuilt only when the recent emoji
    /// change.
    pub(crate) emoji: std::rc::Rc<Vec<PickerItem>>,
    /// Local files of the stickers' pictures, by display file id.
    pub(crate) files: Vec<(i32, std::path::PathBuf)>,
}

impl QuillApp {
    /// The Touch Bar for the app's current state.
    pub(crate) fn touch_bar_model(&self, window: &Window, cx: &App) -> Bar {
        let inputs = BarInputs {
            signed_in: matches!(self.current_auth(), AuthorizationState::Ready),
            locked: self.account.passcode.locked,
            recording: self.recording_active(),
            viewer: self.touch_bar_viewer(),
            song: self.touch_bar_song(),
            composer_selection: self.composer.read(cx).focus_handle(cx).is_focused(window)
                && !self.composer.read(cx).selected_range().is_empty(),
            chat_accepts_stickers_or_text: self.touch_bar_chat_accepts(),
        };
        model::bar(&inputs, || self.touch_bar_pinned())
    }

    fn touch_bar_viewer(&self) -> Option<ViewerBar> {
        let item = self.viewer.state.current()?;
        Some(match item.kind {
            MediaViewerKind::Photo => ViewerBar::Photo,
            kind if kind.is_playable() => {
                let clock = self.viewer.clock.as_ref();
                ViewerBar::Video {
                    playing: clock.is_some_and(|clock| clock.is_playing()),
                    fullscreen: self.viewer.extra.video_fullscreen,
                    position: clock.map_or(0., |clock| clock.elapsed_secs()),
                    duration: clock.map_or(0., |clock| clock.duration_secs()),
                }
            }
            _ => ViewerBar::Other,
        })
    }

    /// tdesktop shows the player for songs only, from start until stop.
    fn touch_bar_song(&self) -> Option<Track> {
        self.playback.playing_audio?;
        let clock = self.playback.clock.as_ref();
        let (can_previous, can_next) = self.playlist_can_move();
        Some(Track {
            playing: clock.is_some_and(|clock| clock.is_playing()),
            can_previous,
            can_next,
            position: clock.map_or(0., |clock| clock.elapsed_secs()),
            duration: clock.map_or(0., |clock| clock.duration_secs()),
        })
    }

    /// tdesktop's `hasActiveChat`: a chat is open and the viewer may send
    /// stickers (`CanSendAnyOf(SendStickers | SendOther)`).
    fn touch_bar_chat_accepts(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let Some(chat) = session.open_chat.and_then(|id| session.chats.get(&id.0)) else {
            return false;
        };
        chat.can_post() && self.send_denial(SendKind::Stickers).is_none()
    }

    fn touch_bar_pinned(&self) -> PinnedPanel {
        let Some(session) = self.session() else {
            return PinnedPanel::default();
        };
        let pin_of = |chat_id: i64, pinned: bool| -> Option<Pin> {
            let chat = session.chats.get(&chat_id)?;
            let saved = session.is_saved_messages(ChatId(chat_id));
            Some(Pin {
                kind: if saved {
                    PinKind::SavedMessages
                } else {
                    PinKind::Chat
                },
                chat_id: Some(chat.id),
                title: chat.title.clone(),
                photo: session
                    .chat_photo_path(chat.id)
                    .map(std::path::PathBuf::from),
                badge: (!saved && chat.is_unread()).then(|| model::Badge {
                    count: (chat.unread_count > 0).then_some(chat.unread_count),
                    muted: chat.is_muted(),
                }),
                online: !saved && session.chat_peer_online(chat),
                pinned,
            })
        };
        let pinned: Vec<Pin> = session
            .pinned_chat_ids(false)
            .into_iter()
            .filter_map(|id| pin_of(id, true))
            .collect();
        let saved = session.my_user_id.map(|my_id| {
            pin_of(my_id, false).unwrap_or(Pin {
                kind: PinKind::SavedMessages,
                chat_id: None,
                title: "Saved Messages".into(),
                photo: None,
                badge: None,
                online: false,
                pinned: false,
            })
        });
        let archive = !session.ordered_archived_chats().is_empty();
        model::pinned_panel(archive, saved, pinned)
    }

    /// What the sticker scrubber lists (tdesktop `updateStickers`); the
    /// emoji are left empty (see `model::emoji_items`).
    pub(crate) fn touch_bar_picker_stickers(&self) -> PickerContents {
        let Some(session) = self.session() else {
            return PickerContents::default();
        };
        let panel = &session.stickers.stickers;
        let library = &session.media.media_library.set_stickers;
        let sections: Vec<StickerSection<'_>> = panel
            .sets
            .iter()
            .take(model::MAX_STICKER_SETS)
            .filter_map(|set| {
                Some(StickerSection {
                    title: if set.title.is_empty() {
                        &set.name
                    } else {
                        &set.title
                    },
                    stickers: library.get(&set.id)?,
                })
            })
            .collect();
        let stickers = model::sticker_items(
            &panel.favorites,
            &panel.recent,
            &sections,
            session.premium_badges_shown(),
        );
        let files = stickers
            .iter()
            .filter_map(|item| match item {
                PickerItem::Sticker(sticker) => sticker.display,
                _ => None,
            })
            .filter_map(|id| {
                let path = session.media.files.get(&id.0)?.usable_path()?;
                Some((id.0, std::path::PathBuf::from(path)))
            })
            .collect();
        PickerContents {
            stickers,
            emoji: std::rc::Rc::default(),
            files,
        }
    }

    /// The picker opened: load the first sets and the pictures it shows
    /// (tdesktop loads them as it builds the items).
    pub(crate) fn touch_bar_picker_shown(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        let sets: Vec<i64> = session
            .stickers
            .stickers
            .sets
            .iter()
            .take(model::MAX_STICKER_SETS)
            .map(|set| set.id)
            .collect();
        let missing: Vec<_> = self
            .touch_bar_picker_stickers()
            .stickers
            .iter()
            .filter_map(|item| match item {
                PickerItem::Sticker(sticker) => sticker.display,
                _ => None,
            })
            .filter(|id| {
                session
                    .media
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .is_none()
            })
            .collect();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.ensure_library_sets(&sets);
            let _ = live.driver.ensure_media_files(&missing);
        }
    }

    /// Run what a touch asked for.
    pub(crate) fn run_touch_bar_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            Command::OpenChat(chat_id) => self.select_listed_chat(chat_id, window, cx),
            Command::OpenSavedMessages => self.open_saved_messages(window, cx),
            Command::OpenArchive => self.open_archive_folder(cx),
            Command::ReorderPinned(ids) => self.touch_bar_reorder(ids, cx),
            Command::Format(format) => self.touch_bar_format(format, window, cx),
            Command::PickerShown => self.touch_bar_picker_shown(),
            Command::SendSticker(sticker) => {
                if self.touch_bar_chat_accepts() {
                    self.send_sticker_pick(
                        sticker.file_id,
                        sticker.emoji,
                        sticker.width,
                        sticker.height,
                        sticker.thumb,
                        cx,
                    );
                }
            }
            Command::InsertEmoji(emoji) => self.touch_bar_insert_emoji(emoji, window, cx),
            Command::PlayerPlayPause => self.toggle_active_playback(cx),
            Command::PlayerPrevious => self.player_step(quill::playlist::Step::Previous, cx),
            Command::PlayerNext => self.player_step(quill::playlist::Step::Next, cx),
            Command::PlayerClose => self.close_player(cx),
            Command::PlayerSeek { fraction, done } => {
                // tdesktop seeks on release (`finishSeeking`).
                if done && let Some(clock) = self.playback.clock.as_ref() {
                    let secs = fraction.clamp(0., 1.) * clock.duration_secs();
                    self.seek_active_to(secs, cx);
                }
            }
            Command::ViewerPlayPause => self.toggle_viewer_video(cx),
            Command::ViewerRotate => match self.viewer.state.current().map(|item| item.kind) {
                Some(MediaViewerKind::Photo) => self.rotate_viewer_photo(cx),
                Some(_) => self.rotate_viewer_video(cx),
                None => {}
            },
            Command::ViewerFullscreen => self.viewer_toggle_fullscreen(window, cx),
            Command::ViewerPictureInPicture => self.open_video_pip(cx),
            // tdesktop seeks the viewer's video while the slider moves
            // (`playbackControlsSeekProgress`), and the player only once
            // it is let go.
            Command::ViewerSeek { fraction, .. } => {
                if let Some(clock) = self.viewer.clock.as_ref() {
                    let secs = fraction.clamp(0., 1.) * clock.duration_secs();
                    self.seek_viewer_to(secs, cx);
                }
            }
        }
    }

    fn touch_bar_reorder(&mut self, ids: Vec<i64>, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_pinned_chat_order(false, ids) {
                self.connection.status_note = format!("pin reorder failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.reorder_pinned_chats(false, &ids);
        }
        cx.notify();
    }

    /// tdesktop sends the formatting shortcut to the focused field.
    fn touch_bar_format(
        &mut self,
        format: FormatCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::composer::ComposerShortcut;
        let shortcut = match format {
            FormatCommand::Bold => ComposerShortcut::Bold,
            FormatCommand::Italic => ComposerShortcut::Italic,
            FormatCommand::Underline => ComposerShortcut::Underline,
            FormatCommand::Strikethrough => ComposerShortcut::Strikethrough,
            FormatCommand::Quote => ComposerShortcut::BlockQuote,
            FormatCommand::Monospace => ComposerShortcut::Monospace,
            FormatCommand::Clear => ComposerShortcut::ClearFormatting,
            FormatCommand::Link => {
                self.composer_link_chord(window, cx);
                return;
            }
        };
        self.run_composer_shortcut(shortcut, window, cx);
    }

    /// tdesktop inserts the emoji at the focused field's cursor and counts
    /// it as recently used; with no field focused nothing happens.
    fn touch_bar_insert_emoji(
        &mut self,
        emoji: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        self.composer.update(cx, |input, cx| {
            input.replace(emoji.as_str(), window, cx);
        });
        self.set_media_pref(move |prefs| prefs.remember_emoji(&emoji), cx);
    }

    /// The colors of the pinned chats panel.
    pub(crate) fn touch_bar_palette(cx: &App) -> Palette {
        use super::chat_theme::{accent, accent_strong, bg_badge_muted, text_on_fill};
        Palette {
            accent: accent(),
            archive: bg_badge_muted(),
            on_fill: text_on_fill(),
            badge: accent_strong(),
            badge_muted: bg_badge_muted(),
            online: cx.theme().success.to_rgb(),
        }
    }
}

/// The disc and letter colors of a chat without a photo.
pub(crate) fn initials_colors(title: &str) -> (Rgba, Rgba) {
    // The Touch Bar is black, so always the dark variant of the kit's
    // identity colors (`message_text::initials_circle`).
    let initials = super::message_text::avatar_initials(title);
    let hue = (gpui_kit::hash(&SharedString::from(initials)) % 12) as f32 * 30.;
    (
        gpui_kit::component::oklch(0.30, 0.05, hue).to_rgb(),
        gpui_kit::component::oklch(0.82, 0.11, hue).to_rgb(),
    )
}

/// The letters drawn on a chat without a photo.
pub(crate) fn initials(title: &str) -> String {
    super::message_text::avatar_initials(title)
}

/// Start mirroring the app into the Touch Bar (macOS with a Touch Bar).
pub(crate) fn install(demo: bool, window: &mut Window, cx: &mut Context<QuillApp>) {
    mac::install(demo, window, cx);
}

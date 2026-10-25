//! The message context menu's extras: media actions (Save As, Copy Image,
//! Show in Folder, GIFs, sticker sets, ...), Report, the "N Seen" /
//! "N Reacted" page, and the admin moderation offered by the delete box.
//! Rows are collected with a sort key from `quill::message_menu::order`
//! (Telegram Desktop's order) and merged into the panel
//! `message_actions.rs` builds.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::context_menu_keys::{ContextMenuItems, MenuKeyTarget, row_key};
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{TestSupportExt, *};
use quill::ids::{ChatId, FileId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::message_menu::{
    MediaAction, MediaFacts, MediaKind, MediaTarget, SeenKind, media_actions, media_target, order,
    reacted_label, read_date_label, read_status_label, seen_kind, seen_label,
};
use quill::state::{Audience, MessageReportStage, Session};
use quill::telegram::envelope::{
    ChannelMemberStatus, MessageActions, MessageContent, MessageSender, ReactionType, ReportOption,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Which page of the message menu is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum MessageMenuPage {
    #[default]
    Main,
    /// A song's "Save to..." destinations.
    SaveTo,
    /// "Add to Sticker Set" / "Add to Emoji Set": the sets the user
    /// created.
    AddToSet { emoji: bool },
    /// Who saw / reacted to the message.
    Audience,
    /// The page of the poll option the menu was opened over (tdesktop
    /// `FillPollOptionPage`).
    PollOption,
}

/// UI state of the message menu's extras.
pub(super) struct MessageMenuUi {
    pub page: MessageMenuPage,
    /// The report dialog (`DialogKind::MessageReport`) is open.
    pub report_open: bool,
    /// The sticker set dialog (`DialogKind::StickerSet`) is open.
    pub sticker_set_open: bool,
    /// The "who reacted" tab (`None` = All).
    pub audience_tab: Option<quill::telegram::envelope::ReactionType>,
    /// Details of a report (`reportChatResultTextRequired`).
    pub report_text: Entity<TextareaState>,
}

impl MessageMenuUi {
    pub(super) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            page: MessageMenuPage::Main,
            report_open: false,
            sticker_set_open: false,
            audience_tab: None,
            report_text: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(tr(keys::Q_CHAT_ADD_COMMENT))
                    .auto_grow(2, 5)
                    .submit_on_enter(false)
            }),
        }
    }
}

/// What the admin checkboxes of the delete box may offer for a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModerationOffer {
    pub chat_id: ChatId,
    pub user_id: i64,
    pub user_name: String,
    /// Which checkboxes apply (`quill::moderation::moderate_options`).
    pub options: quill::moderation::ModerateOptions,
    /// Banning can be softened to a restriction (supergroups only;
    /// tdesktop's expander under "Ban").
    pub can_restrict_instead: bool,
}

/// One row of the menu with its sort key.
pub(super) type MenuRow = (u8, AnyElement);

/// A clickable menu row (icon + label), like the rows `message_actions.rs`
/// builds with its `item!` macro.
pub(super) fn menu_row(
    order: u8,
    icon: IconName,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    danger: bool,
    items: &ContextMenuItems,
    cx: &mut Context<QuillApp>,
    on_click: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> MenuRow {
    let label: SharedString = label.into();
    let row_hover = cx.theme().accent;
    (
        order,
        div()
            .id(id)
            .test_support()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .text_color(if danger { danger_bright() } else { text_menu() })
            .hover(|style| style.bg(row_hover))
            .menu_key_target(row_key(order), items, cx)
            .role(gpui_kit::Role::MenuItem)
            .aria_label(label.clone())
            .child(Icon::new(icon).size(px(16.)))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            .into_any_element(),
    )
}

/// A muted, non-clickable line (the date row, the no-forwards note).
pub(super) fn info_row(
    order: u8,
    id: &'static str,
    icon: Option<IconName>,
    text: impl Into<SharedString>,
) -> MenuRow {
    (
        order,
        div()
            .id(id)
            .test_support()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_1p5()
            .text_xs()
            .text_color(text_muted())
            .when_some(icon, |this, icon| this.child(Icon::new(icon).size(px(16.))))
            .child(text.into())
            .into_any_element(),
    )
}

/// A hairline between the actions and the information rows.
pub(super) fn menu_separator(order: u8) -> MenuRow {
    (
        order,
        div()
            .h(px(1.))
            .mx_3()
            .my_1()
            .bg(border())
            .into_any_element(),
    )
}

/// Decode an image file and encode it as PNG for the clipboard.
fn png_bytes_from_file(path: &Path) -> Result<Vec<u8>, &'static str> {
    let rgba = super::image_decode::decode_file(path, super::image_decode::DecodeSize::FULL)
        .map_err(|_| tr(keys::Q_CHAT_COULDNT_COPY_THIS_IMAGE))?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|_| tr(keys::Q_CHAT_COULDNT_COPY_THIS_IMAGE))?;
    Ok(bytes.into_inner())
}

/// The local file behind a media message, if it is on disk. A photo
/// resolves to its largest size that has been downloaded.
fn local_media_path(
    session: &Session,
    message: &MessageContent,
    target: &MediaTarget,
) -> Option<PathBuf> {
    if let MessageContent::Photo(photo) = message {
        let mut sizes: Vec<_> = photo.sizes.iter().collect();
        sizes.sort_by_key(|size| std::cmp::Reverse(i64::from(size.width) * i64::from(size.height)));
        return sizes
            .into_iter()
            .find_map(|size| session.file(size.file_id)?.usable_path().map(PathBuf::from));
    }
    session
        .file(target.file_id)?
        .usable_path()
        .map(PathBuf::from)
}

impl QuillApp {
    /// What the menu knows about a message's media right now.
    fn menu_media_facts(
        &self,
        session: &Session,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        target: &MediaTarget,
        actions: Option<MessageActions>,
    ) -> MediaFacts {
        let protected = session.chat_has_protected_content(chat_id);
        let file = session.file(target.file_id);
        let downloading = session
            .media
            .downloads
            .is_downloading(target.file_id.0, &session.media.files)
            || session.requests.has_download(target.file_id);
        let content = quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        );
        // A set-less, sent sticker may join one of the user's own sets
        // (tdesktop `AddAddToOwnedSetAction`: lottie fits both, ≤100 px is
        // an emoji; nothing while the message is still sending).
        let joins = match content {
            MessageContent::Sticker(sticker) if sticker.set_id == 0 && !message.pending => {
                let (to_stickers, to_emoji) = quill::sticker_set_box::joinable_kinds(
                    sticker.format == quill::telegram::envelope::StickerFormat::Tgs,
                    sticker.width,
                    sticker.height,
                );
                (
                    to_stickers && !session.owned_sticker_sets(false).is_empty(),
                    to_emoji && !session.owned_sticker_sets(true).is_empty(),
                )
            }
            _ => (false, false),
        };
        MediaFacts {
            downloading,
            local: local_media_path(session, content, target).is_some(),
            can_save: !protected && actions.is_none_or(|a| a.can_be_saved),
            set_installed: session
                .stickers
                .stickers
                .sets
                .iter()
                .any(|set| set.id == target.set_id),
            favorite: (target.kind == MediaKind::Sticker
                && (session.stickers.stickers.installed_loaded
                    || !session.stickers.stickers.favorites.is_empty()))
            .then(|| {
                session
                    .stickers
                    .stickers
                    .favorites
                    .iter()
                    .any(|s| s.file_id == target.file_id)
            }),
            gif_saved: session.stickers.gifs.loaded.then(|| {
                session
                    .stickers
                    .gifs
                    .animations
                    .iter()
                    .any(|item| item.file_id == target.file_id)
            }),
            in_saved_messages: session.is_saved_messages(chat_id),
            in_profile: session.profile_audio_known(target.file_id),
            joins_sticker_set: joins.0,
            joins_emoji_set: joins.1,
            tone_ok: message.self_destruct.is_none()
                && quill::message_menu::tone_offered(
                    target,
                    file.map_or(0, |f| f.display_size()),
                    session.settings.saved_notification_sounds.len(),
                    session.messages.tone_limits,
                ),
        }
    }

    /// The media block of the menu (`AddPhotoActions`,
    /// `AddDocumentActions`).
    pub(super) fn menu_media_rows(
        &self,
        chat_id: ChatId,
        message: &quill::state::HistoryMessage,
        actions: Option<MessageActions>,
        cx: &mut Context<Self>,
    ) -> Vec<MenuRow> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let content = quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        );
        let Some(target) = media_target(content) else {
            return Vec::new();
        };
        let facts = self.menu_media_facts(session, chat_id, message, &target, actions);
        let message_id = message.id;
        let finder = cfg!(target_os = "macos");
        let mut rows = Vec::new();
        for action in media_actions(&target, &facts) {
            let sort = action.sort_key();
            let icon = media_action_icon(action);
            let label = action.label(finder);
            let target = target.clone();
            rows.push(menu_row(
                sort,
                icon,
                action.id(),
                label,
                false,
                &self.frame.context_menu_items,
                cx,
                move |this, window, cx| {
                    let _ = window;
                    match action {
                        MediaAction::CancelDownload => {
                            this.cancel_media_download(target.file_id, cx)
                        }
                        MediaAction::OpenGif => this.open_media_viewer(chat_id, message_id, cx),
                        MediaAction::SaveGif => this.save_message_gif(target.file_id, cx),
                        MediaAction::ViewStickerSet { .. } => {
                            this.view_message_sticker_set(target.set_id, cx)
                        }
                        MediaAction::ToggleFavorite { remove } => {
                            this.favorite_message_sticker(target.file_id, !remove, cx)
                        }
                        MediaAction::ShowInFolder => {
                            let file_id = this
                                .menu_media_local(chat_id, message_id)
                                .map(|(id, _)| id)
                                .unwrap_or(target.file_id);
                            this.reveal_downloaded_file(file_id, cx)
                        }
                        MediaAction::SaveForNotifications => {
                            this.save_message_tone(target.file_id, cx)
                        }
                        MediaAction::SaveTo => {
                            this.message_ui.menu_ui.page = MessageMenuPage::SaveTo;
                            cx.notify();
                            return;
                        }
                        MediaAction::AddToSet { emoji } => {
                            this.message_ui.menu_ui.page = MessageMenuPage::AddToSet { emoji };
                            cx.notify();
                            return;
                        }
                        MediaAction::SaveAs => this.save_message_media_as(chat_id, message_id, cx),
                        MediaAction::CopyImage => this.copy_message_image(chat_id, message_id, cx),
                        MediaAction::CopyFilename => {
                            if let Some(name) = target.copy_name.clone() {
                                cx.write_to_clipboard(ClipboardItem::new_string(name));
                                this.connection
                                    .set_note(StatusNote::info(tr(keys::Q_CHAT_FILENAME_COPIED)));
                            }
                        }
                    }
                    this.message_ui.menu = None;
                    cx.notify();
                },
            ));
        }
        rows
    }

    /// The on-disk file of the media message: its file id and path.
    pub(super) fn menu_media_local(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Option<(FileId, PathBuf)> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        let content = quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        );
        let target = media_target(content)?;
        if let MessageContent::Photo(photo) = content {
            let mut sizes: Vec<_> = photo.sizes.iter().collect();
            sizes.sort_by_key(|s| std::cmp::Reverse(i64::from(s.width) * i64::from(s.height)));
            return sizes.into_iter().find_map(|size| {
                let path = session.file(size.file_id)?.usable_path()?;
                Some((size.file_id, PathBuf::from(path)))
            });
        }
        let path = session.file(target.file_id)?.usable_path()?;
        Some((target.file_id, PathBuf::from(path)))
    }

    /// "Save As...": pick a place, then copy the downloaded file there.
    pub(super) fn save_message_media_as(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let target = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0)?.messages.get(&message_id.0))
            .and_then(|m| {
                media_target(quill::telegram::envelope::effective_content(
                    &m.content,
                    m.ephemeral.as_ref(),
                ))
            });
        let Some(target) = target else { return };
        let Some((_, path)) = self.menu_media_local(chat_id, message_id) else {
            // Start the download so a second try finds the file.
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.ensure_media_files(&[target.file_id]);
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DOWNLOADING_CHOOSE_SAVE_AS_AGAIN,
            )));
            cx.notify();
            return;
        };
        let local_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let suggested = quill::message_menu::suggested_save_name(&target, &local_name);
        let dir = quill::media_viewer::downloads_dir().unwrap_or_else(|| PathBuf::from("."));
        let picker = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = picker.await else {
                return;
            };
            let copied = cx
                .background_executor()
                .spawn(async move { std::fs::copy(&path, &dest).map(|_| dest) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection.set_note(match copied {
                    Ok(dest) => StatusNote::info(format!(
                        "saved to {}",
                        dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                    )),
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_CHAT_COULDNT_SAVE,
                        &[("err", &err.to_string())],
                    )),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// "Copy Image": the photo as PNG on the clipboard.
    pub(super) fn copy_message_image(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        let Some((_, path)) = self.menu_media_local(chat_id, message_id) else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DOWNLOAD_THE_PHOTO_FIRST_TO,
            )));
            cx.notify();
            return;
        };
        // Decode and encode off the UI thread; the clipboard write needs it.
        self.run_off_ui_then(
            move || png_bytes_from_file(&path),
            |this, result, cx| {
                match result {
                    Ok(bytes) => {
                        cx.write_to_clipboard(ClipboardItem::new_image(
                            &gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes),
                        ));
                        this.connection
                            .set_note(StatusNote::info(tr(keys::Q_CHAT_PHOTO_COPIED)));
                    }
                    Err(note) => this.connection.set_note(StatusNote::error(note)),
                }
                cx.notify();
            },
            cx,
        );
    }

    /// "Add to GIFs" (`addSavedAnimation`).
    fn save_message_gif(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let sent = live.driver.set_gif_saved(file_id, true);
            self.connection.set_note(match sent {
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_SAVING_GIF)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SAVE_GIF)),
            });
            if sent.is_ok() {
                self.note_gif_replaced(cx);
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_GIFS_SAVE_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    /// "Save for Notifications" (`addSavedNotificationSound`).
    fn save_message_tone(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.save_notification_tone(file_id) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_SAVING_SOUND)),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SAVE_THE_SOUND)),
                });
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_SOUND_ADDED)));
        }
        cx.notify();
    }

    /// "Add to Favorites" / "Remove from Favorites".
    pub(in crate::ui) fn favorite_message_sticker(
        &mut self,
        file_id: FileId,
        favorite: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let sent = live.driver.set_favorite_sticker(file_id, favorite);
            self.connection.set_note(match sent {
                Ok(_) if favorite => StatusNote::silent(tr(keys::Q_CHAT_ADDED_TO_FAVORITES)),
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_REMOVED_FROM_FAVORITES)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_CHANGE_FAVORITES)),
            });
            if favorite && sent.is_ok() {
                self.note_faved_replaced(file_id, cx);
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_FAVORITES_CHANGE_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    /// "Cancel Upload" on a message still being sent.
    pub(super) fn cancel_message_upload(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.cancel_upload(chat_id, message_id) {
                    Ok(_) => {
                        self.begin_vanish(chat_id, &[message_id]);
                        StatusNote::silent(tr(keys::Q_CHAT_UPLOAD_CANCELED))
                    }
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_CANCEL_THE_UPLOAD)),
                });
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_UPLOADS_CANCEL_WITH_LIVE,
            )));
        }
        cx.notify();
    }

    // ----------------------------------------------------------------
    // Song: "Save to..." page.
    // ----------------------------------------------------------------

    // ----------------------------------------------------------------
    // Seen / reacted.
    // ----------------------------------------------------------------

    // ----------------------------------------------------------------
    // Report.
    // ----------------------------------------------------------------

    // ----------------------------------------------------------------
    // Moderation.
    // ----------------------------------------------------------------
}

/// The icon of a media row.
fn media_action_icon(action: MediaAction) -> IconName {
    match action {
        MediaAction::CancelDownload => IconName::X,
        MediaAction::OpenGif => IconName::ExternalLink,
        MediaAction::SaveGif => IconName::Plus,
        MediaAction::ViewStickerSet { .. } | MediaAction::AddToSet { .. } => IconName::Sticker,
        MediaAction::ToggleFavorite { remove: false } => IconName::Star,
        MediaAction::ToggleFavorite { remove: true } => IconName::StarOff,
        MediaAction::ShowInFolder => IconName::FolderOpen,
        MediaAction::SaveForNotifications => IconName::BellPlus,
        MediaAction::SaveTo | MediaAction::SaveAs => IconName::Download,
        MediaAction::CopyImage | MediaAction::CopyFilename => IconName::Copy,
    }
}

/// Sort the rows by their key and stack them.
pub(super) fn stack_rows(mut rows: Vec<MenuRow>) -> Vec<AnyElement> {
    rows.sort_by_key(|(order, _)| *order);
    rows.into_iter().map(|(_, row)| row).collect()
}

crate::ui::shell::register_dialogs! {
    /// The message menu's Report flow.
    MessageReport => DialogSpec::new(
        5600,
        |app| app.message_ui.menu_ui.report_open,
        QuillApp::build_message_report_dialog,
    ),
}

mod delete_member_reaction;
pub(in crate::ui) mod item_rows;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod media_order_tests;
mod menu_add_to_set_rows;
mod menu_save_to_rows;
pub(in crate::ui) mod origin_rows;

#[cfg(test)]
mod tests {
    #[test]
    fn copy_image_encodes_a_png_and_refuses_garbage() {
        let dir = std::env::temp_dir().join(format!("quill-copy-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("pixel.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&good)
            .unwrap();
        let bytes = super::png_bytes_from_file(&good).expect("png");
        assert_eq!(&bytes[..4], b"\x89PNG");
        let bad = dir.join("broken.png");
        std::fs::write(&bad, b"not an image").unwrap();
        assert!(super::png_bytes_from_file(&bad).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Copy Image reads HEIC through the shared decoder (`image::open` could not).
    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn copy_image_reads_heic() {
        let heic = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/images/halves.heic");
        let bytes = super::png_bytes_from_file(&heic).expect("png from heic");
        assert_eq!(&bytes[..4], b"\x89PNG");
    }
}

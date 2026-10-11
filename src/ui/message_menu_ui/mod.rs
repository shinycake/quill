//! The message context menu's extras: media actions (Save As, Copy Image,
//! Show in Folder, GIFs, sticker sets, ...), Report, the "N Seen" /
//! "N Reacted" page, and the admin moderation offered by the delete box.
//! Rows are collected with a sort key from `quill::message_menu::order`
//! (Telegram Desktop's order) and merged into the panel
//! `message_actions.rs` builds.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::message_menu::{
    MediaAction, MediaFacts, MediaKind, MediaTarget, SeenKind, media_actions, media_target, order,
    reacted_label, read_date_label, read_status_label, seen_kind, seen_label,
};
use quill::state::{Audience, MessageReportStage, Session, StickerSetViewStage};
use quill::sticker_set_box::{
    ARCHIVED_NOTE, SetKind, can_archive, copied_note, set_link, share_label,
};
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
    /// Who saw / reacted to the message.
    Audience,
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
                    .placeholder("Add Comment")
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
    id: &'static str,
    label: impl Into<SharedString>,
    danger: bool,
    cx: &mut Context<QuillApp>,
    on_click: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> MenuRow {
    let label: SharedString = label.into();
    let row_hover = cx.theme().accent;
    (
        order,
        div()
            .id(id)
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
    let rgba = image::open(path)
        .map_err(|_| "couldn't copy this image")?
        .to_rgba8();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|_| "couldn't copy this image")?;
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
        let downloading = session.media.downloading.contains(&target.file_id.0)
            || session.requests.has_download(target.file_id)
            || file.is_some_and(|f| f.local.is_downloading_active);
        let content = quill::telegram::envelope::effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        );
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
        for (index, action) in media_actions(&target, &facts).into_iter().enumerate() {
            let sort = order::MEDIA + index as u8;
            let icon = media_action_icon(action);
            let label = action.label(finder);
            let target = target.clone();
            rows.push(menu_row(
                sort,
                icon,
                action.id(),
                label,
                false,
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
                        MediaAction::SaveAs => this.save_message_media_as(chat_id, message_id, cx),
                        MediaAction::CopyImage => this.copy_message_image(chat_id, message_id, cx),
                        MediaAction::CopyFilename => {
                            if let Some(name) = target.copy_name.clone() {
                                cx.write_to_clipboard(ClipboardItem::new_string(name));
                                this.connection.status_note = "filename copied".into();
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
            self.connection.status_note =
                "downloading… choose Save As again when it finishes".into();
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
                this.connection.status_note = match copied {
                    Ok(dest) => format!(
                        "saved to {}",
                        dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                    ),
                    Err(err) => format!("couldn't save: {err}"),
                };
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
            self.connection.status_note = "download the photo first to copy it".into();
            cx.notify();
            return;
        };
        match png_bytes_from_file(&path) {
            Ok(bytes) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    bytes,
                )));
                self.connection.status_note = "photo copied".into();
            }
            Err(note) => self.connection.status_note = note.into(),
        }
        cx.notify();
    }

    /// "Add to GIFs" (`addSavedAnimation`).
    fn save_message_gif(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_gif_saved(file_id, true) {
                Ok(_) => "saving GIF…".into(),
                Err(_) => "could not save GIF".into(),
            };
        } else {
            self.connection.status_note = "demo — GIFs save with live TDLib".into();
        }
        cx.notify();
    }

    /// "Save for Notifications" (`addSavedNotificationSound`).
    fn save_message_tone(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.save_notification_tone(file_id) {
                Ok(_) => "saving sound…".into(),
                Err(_) => "could not save the sound".into(),
            };
        } else {
            self.connection.status_note = "Sound added!".into();
        }
        cx.notify();
    }

    /// "Add to Favorites" / "Remove from Favorites".
    fn favorite_message_sticker(
        &mut self,
        file_id: FileId,
        favorite: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_favorite_sticker(file_id, favorite)
            {
                Ok(_) if favorite => "added to favorites".into(),
                Ok(_) => "removed from favorites".into(),
                Err(_) => "could not change favorites".into(),
            };
        } else {
            self.connection.status_note = "demo — favorites change with live TDLib".into();
        }
        cx.notify();
    }

    /// "View Sticker Set" / "Add Stickers": the set in a dialog.
    pub(super) fn view_message_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
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
    fn toggle_viewed_sticker_set(&mut self, set_id: i64, install: bool, cx: &mut Context<Self>) {
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

    pub(super) fn close_sticker_set_dialog(&mut self, cx: &mut Context<Self>) {
        self.message_ui.menu_ui.sticker_set_open = false;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.sticker_set_view = None;
        }
        cx.notify();
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
                    stickers,
                } => {
                    let count = stickers.len();
                    let mut grid = div().flex().flex_wrap().gap_1().w(px(380.));
                    for (index, item) in stickers.iter().enumerate() {
                        let still = this.panel_still(item);
                        grid = grid.child(
                            div()
                                .id(("sticker-set-cell", index as u64))
                                .size(px(72.))
                                .p_1()
                                .flex()
                                .items_center()
                                .justify_center()
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
                                }),
                        );
                    }
                    let set_id = view.set_id;
                    let kind = SetKind::of(is_emoji);
                    let more = set_link(&name, kind).map(|link| {
                        let owner = cx.entity().downgrade();
                        let archivable = can_archive(installed, kind);
                        Button::new("sticker-set-more")
                            .icon(IconName::Ellipsis)
                            .ghost()
                            .tooltip("More")
                            .accessibility_label("More")
                            .dropdown_menu(move |menu, _, _| {
                                let share = owner.clone();
                                let copy = owner.clone();
                                let archive = owner.clone();
                                let (share_link, copy_link) = (link.clone(), link.clone());
                                let menu = menu
                                    .item(PopupMenuItem::new(share_label(kind)).on_click(
                                        move |_, window, cx| {
                                            let link = share_link.clone();
                                            let _ = share.update(cx, |this, cx| {
                                                this.share_sticker_set_link(link, window, cx);
                                            });
                                        },
                                    ))
                                    .item(PopupMenuItem::new("Copy Link").on_click(
                                        move |_, _, cx| {
                                            let link = copy_link.clone();
                                            let _ = copy.update(cx, |this, cx| {
                                                this.copy_sticker_set_link(link, kind, cx);
                                            });
                                        },
                                    ));
                                if archivable {
                                    menu.item(PopupMenuItem::new("Archive Stickers").on_click(
                                        move |_, window, cx| {
                                            let _ = archive.update(cx, |this, cx| {
                                                this.archive_viewed_sticker_set(set_id, window, cx);
                                            });
                                        },
                                    ))
                                } else {
                                    menu
                                }
                            })
                    });
                    let footer = div()
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
                        .child(
                            Button::new("sticker-set-toggle")
                                .label(if installed {
                                    "Remove Stickers".to_string()
                                } else {
                                    format!("Add {count} Stickers")
                                })
                                .when(installed, |button| button.danger())
                                .when(!installed, |button| button.primary())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_viewed_sticker_set(set_id, !installed, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::StickerSet,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .into_any_element();
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

    /// "Cancel Upload" on a message still being sent.
    pub(super) fn cancel_message_upload(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.cancel_upload(chat_id, message_id) {
                Ok(_) => {
                    self.begin_vanish(chat_id, &[message_id]);
                    "upload canceled".into()
                }
                Err(_) => "could not cancel the upload".into(),
            };
        } else {
            self.connection.status_note = "demo — uploads cancel with live TDLib".into();
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
        MediaAction::ViewStickerSet { .. } => IconName::Sticker,
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

    /// "View Sticker Set" / "Add Stickers" from a sticker message.
    StickerSet => DialogSpec::new(
        5700,
        |app| app.message_ui.menu_ui.sticker_set_open,
        QuillApp::build_sticker_set_dialog,
    ),
}

mod delete_member_reaction;
mod menu_save_to_rows;

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
}

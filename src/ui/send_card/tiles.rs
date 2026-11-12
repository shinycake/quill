//! The send card's previews: each photo and video message drawn the way it
//! will be sent (tdesktop's album preview in the send box), with its
//! controls on a dark pill that reads over any picture, and the files as
//! rows with a thumbnail, name and size.
//!
//! A tile's controls are buttons, one click each: Edit, the spoiler toggle,
//! the cover, Remove. Only a tile too narrow for them folds them into one
//! menu, and a video with a cover has a menu for changing or clearing it.
#![deny(clippy::indexing_slicing)]

use super::layout::{MESSAGE_GAP, MOSAIC_WIDTH, TrayLayout, message_tiles};
use crate::ui::app::QuillApp;
use crate::ui::composer_sends::TrayItem;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::{black, white};
use quill::album::{AlbumRect, layout_bounds};
use quill::composer::{AttachmentKind, ComposerAttachment, is_gif_path, session_allows_cover};
use quill::lang::{keys, tr, tr_with};

/// Rounding of a tile; a mosaic's tiles share it.
const TILE_RADIUS: f32 = 8.;
/// The corners a mosaic's tiles share with each other.
const INNER_RADIUS: f32 = 2.;
/// A file row's thumbnail.
const FILE_THUMB: f32 = 40.;
/// The room one control takes on a tile's pill, and the room the pill
/// keeps around them: its own padding, its inset from the tile's edge and
/// some of the picture left in view beside it.
const CONTROL_WIDTH: i32 = 22;
const PILL_CHROME: i32 = 32;

/// One thing a tile's controls do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TileAction {
    Edit(EditKind),
    /// tdesktop "Hide with spoiler", with whether it is hidden now.
    Spoiler {
        hidden: bool,
    },
    /// The cover of a video, with whether one is chosen.
    Cover {
        chosen: bool,
    },
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    Photo,
    Video,
}

/// How a tile `width` wide shows `actions`: each on its own button, or all
/// in one menu when they don't fit (a menu of one is that one button).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TileControls {
    Buttons,
    Menu,
}

impl TileControls {
    pub(super) fn for_tile(width: i32, actions: usize) -> Self {
        let needed = PILL_CHROME + CONTROL_WIDTH * i32::try_from(actions).unwrap_or(i32::MAX / 64);
        if actions < 2 || width >= needed {
            Self::Buttons
        } else {
            Self::Menu
        }
    }
}

impl QuillApp {
    #[inline(never)]
    pub(super) fn send_card_media(
        &self,
        layout: &TrayLayout,
        max_height: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if layout.media.is_empty() {
            return None;
        }
        let max_width = layout.max_width();
        // Messages that do not fit beside each other wrap: the box scrolls
        // down, never sideways.
        let mut strip = div()
            .flex()
            .flex_wrap()
            .justify_center()
            .items_start()
            .gap(px(MESSAGE_GAP as f32))
            .max_w(px(MOSAIC_WIDTH as f32));
        for message in &layout.media {
            let sizes: Vec<(i32, i32)> = message
                .iter()
                .map(|index| self.send_card_tile_size(*index))
                .collect();
            let tiles = message_tiles(&sizes, max_height, max_width);
            let (width, height) = layout_bounds(&tiles);
            let mut block = div()
                .relative()
                .flex_none()
                .w(px(width as f32))
                .h(px(height as f32));
            for (tile, index) in tiles.iter().zip(message) {
                let corners = TileCorners::of(tile, (width, height));
                block = block.child(self.send_card_tile(*index, tile, corners, cx));
            }
            let first = message.first().copied().unwrap_or_default();
            // An album taller than its room scrolls (`narrowest_album`). The
            // scroll area fills what it is given, so it is given the
            // message's own size.
            strip = strip.child(
                div()
                    .id(("send-card-message", first as u64))
                    .debug_selector(move || format!("send-card-message-{first}"))
                    .flex_none()
                    .w(px(width as f32))
                    .h(px(height.min(max_height) as f32))
                    .rounded(px(TILE_RADIUS))
                    .child(block)
                    .overflow_y_scrollbar(),
            );
        }
        Some(strip.into_any_element())
    }

    /// The picture size a tile is laid out by: the photo's, the video's
    /// first frame, or a guess while it is read.
    fn send_card_tile_size(&self, index: usize) -> (i32, i32) {
        let Some(attachment) = self.pending_attachments().get(index) else {
            return (4, 3);
        };
        let size = match attachment.kind {
            AttachmentKind::VideoNote => Some((1, 1)),
            AttachmentKind::Video => self
                .composer_sends
                .reads
                .previews
                .done(&attachment.path)
                .and_then(|preview| preview.frame.as_ref())
                .map(|frame| (frame.width, frame.height)),
            _ => self.tray_photo_size(attachment),
        };
        size.map_or((4, 3), |(width, height)| {
            (
                i32::try_from(width).unwrap_or(i32::MAX),
                i32::try_from(height).unwrap_or(i32::MAX),
            )
        })
    }

    #[inline(never)]
    fn send_card_tile(
        &self,
        index: usize,
        tile: &AlbumRect,
        corners: TileCorners,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(attachment) = self.pending_attachments().get(index) else {
            return div().into_any_element();
        };
        let round = attachment.kind == AttachmentKind::VideoNote;
        let reads = &self.composer_sends.reads;
        let preview = reads.previews.done(&attachment.path);
        let reading = reads.media.is_reading(&attachment.path)
            || reads.sound.is_reading(&attachment.path)
            || reads.previews.is_reading(&attachment.path);
        let picture = match attachment.kind {
            AttachmentKind::Photo
                if !quill::composer::photo_needs_jpeg(attachment.photo_file()) =>
            {
                Some(attachment.photo_file().to_path_buf())
            }
            // A chosen cover is what the chat will show, so the tile shows it.
            AttachmentKind::Video | AttachmentKind::VideoNote => {
                attachment.extras.cover.clone().or_else(|| {
                    preview
                        .and_then(|preview| preview.frame.as_ref())
                        .map(|frame| frame.path.clone())
                })
            }
            _ => None,
        };
        let badge = if attachment.sends_as_animation(
            self.pending_attachments().len(),
            self.composer_group_media_effective(),
        ) {
            Some(tr(keys::Q_OPS_GIF).to_string())
        } else {
            let duration = preview
                .and_then(|preview| preview.duration)
                .map(quill::voice::format_voice_duration);
            match (attachment.extras.cover.is_some(), duration) {
                (true, Some(duration)) => Some(format!("{} · {duration}", tr(keys::Q_OPS_COVER))),
                (true, None) => Some(tr(keys::Q_OPS_COVER).to_string()),
                (false, duration) => duration,
            }
        };
        let theme = cx.theme();
        let (muted_bg, muted_fg) = (theme.muted, theme.muted_foreground);
        let icon = match attachment.kind {
            AttachmentKind::Photo => IconName::Image,
            _ => IconName::Film,
        };
        div()
            .id(("send-card-tile", index as u64))
            .absolute()
            .left(px(tile.x as f32))
            .top(px(tile.y as f32))
            .w(px(tile.width as f32))
            .h(px(tile.height as f32))
            .overflow_hidden()
            .map(|this| corners.round(this, round))
            .bg(muted_bg)
            .map(|this| match picture {
                Some(path) => this.child(
                    img(path)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .map(|this| corners.round(this, round)),
                ),
                None => this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(icon).size(px(24.)).text_color(muted_fg)),
                ),
            })
            .when(attachment.spoiler, |this| {
                this.child(spoiler_veil(corners, round))
            })
            .when(reading, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .p_1p5()
                                .rounded_full()
                                .bg(black().opacity(0.5))
                                .child(Spinner::new().color(white())),
                        ),
                )
            })
            .when_some(badge.filter(|_| !round), |this, badge| {
                this.child(
                    div()
                        .absolute()
                        .bottom_1()
                        .left_1()
                        .px_1p5()
                        .rounded_md()
                        .bg(black().opacity(0.55))
                        .text_xs()
                        .font_medium()
                        .text_color(white())
                        .child(badge),
                )
            })
            .child(self.send_card_tile_controls(index, attachment, tile.width, cx))
            .into_any_element()
    }

    /// The tile's controls on one dark pill.
    #[inline(never)]
    fn send_card_tile_controls(
        &self,
        index: usize,
        attachment: &ComposerAttachment,
        width: i32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actions = self.send_card_tile_actions(attachment);
        let Some(item) = self.tray_item(index) else {
            return div().into_any_element();
        };
        // The pill takes its own presses and hover: none reaches the tile
        // or the box under it.
        let pill = div()
            .occlude()
            .absolute()
            .top_1()
            .right_1()
            .flex()
            .items_center()
            .p(px(2.))
            .rounded_full()
            .bg(black().opacity(0.55));
        match TileControls::for_tile(width, actions.len()) {
            TileControls::Buttons => pill
                .children(actions.iter().map(|action| {
                    self.send_card_tile_button(&item, index, attachment, *action, cx)
                }))
                .into_any_element(),
            TileControls::Menu => {
                let owner = cx.entity().downgrade();
                pill.child(
                    on_media(Button::new(("send-card-more", index as u64)), cx)
                        .icon(IconName::Ellipsis)
                        .tooltip(tr(keys::Q_CHAT_MORE))
                        .dropdown_menu(move |popup, _, _| {
                            tile_menu(popup, item.clone(), &actions, owner.clone())
                        }),
                )
                .into_any_element()
            }
        }
    }

    /// What a tile's controls offer, in the order they show.
    pub(super) fn send_card_tile_actions(
        &self,
        attachment: &ComposerAttachment,
    ) -> Vec<TileAction> {
        let edit = match attachment.kind {
            AttachmentKind::Photo => Some(EditKind::Photo),
            // A `.gif` file is an animation already: nothing to edit.
            AttachmentKind::Video if !is_gif_path(&attachment.path) => Some(EditKind::Video),
            _ => None,
        };
        let media = matches!(
            attachment.kind,
            AttachmentKind::Photo | AttachmentKind::Video
        );
        // Paid media is blurred by Telegram itself (tdesktop `hasPrice()`).
        let spoiler = media && !self.tray_paid_price_set();
        // A video that goes out as a GIF has no cover.
        let cover = attachment.kind == AttachmentKind::Video
            && edit.is_some()
            && !attachment.sends_as_animation(
                self.pending_attachments().len(),
                self.composer_group_media_effective(),
            )
            && self.session().is_some_and(|session| {
                session
                    .open_chat_id()
                    .is_some_and(|chat| session_allows_cover(session, chat))
            });
        let mut actions = Vec::with_capacity(4);
        actions.extend(edit.map(TileAction::Edit));
        if spoiler {
            actions.push(TileAction::Spoiler {
                hidden: attachment.spoiler,
            });
        }
        if cover {
            actions.push(TileAction::Cover {
                chosen: attachment.extras.cover.is_some(),
            });
        }
        actions.push(TileAction::Remove);
        actions
    }

    fn send_card_tile_button(
        &self,
        item: &TrayItem,
        index: usize,
        attachment: &ComposerAttachment,
        action: TileAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = index as u64;
        match action {
            TileAction::Edit(kind) => {
                // Its edit is still being saved.
                let rendering = attachment.extras.photo_render().is_some();
                on_media(Button::new(("send-card-edit", id)), cx)
                    .map(|button| {
                        if rendering {
                            button
                                .icon(Spinner::new().color(white()))
                                .debug_selector(move || {
                                    format!("composer-attach-rendering-{index}")
                                })
                        } else {
                            button.icon(IconName::Pencil)
                        }
                    })
                    .tooltip(kind.label())
                    .on_click({
                        let item = item.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.edit_send_card_item(&item, kind, window, cx)
                        })
                    })
                    .into_any_element()
            }
            TileAction::Spoiler { hidden } => chosen_mark(
                hidden,
                on_media_state(Button::new(("send-card-spoiler", id)), hidden, cx)
                    .icon(IconName::EyeOff)
                    .debug_selector(move || format!("send-card-spoiler-{index}"))
                    .tooltip(spoiler_label(hidden))
                    .on_click({
                        let item = item.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.act_on_tile(&item, |this, index| {
                                this.toggle_attachment_spoiler(index, cx)
                            })
                        })
                    }),
            ),
            TileAction::Cover { chosen: false } => {
                on_media(Button::new(("send-card-cover", id)), cx)
                    .icon(IconName::Image)
                    .tooltip(tr(keys::Q_OPS_EDIT_COVER))
                    .on_click({
                        let item = item.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.act_on_tile(&item, |this, index| {
                                this.pick_attachment_cover(index, cx)
                            })
                        })
                    })
                    .into_any_element()
            }
            TileAction::Cover { chosen: true } => {
                let owner = cx.entity().downgrade();
                chosen_mark(
                    true,
                    on_media_state(Button::new(("send-card-cover", id)), true, cx)
                        .icon(IconName::Image)
                        .tooltip(tr(keys::Q_OPS_COVER))
                        .dropdown_menu({
                            let item = item.clone();
                            move |popup, _, _| cover_items(popup, item.clone(), true, owner.clone())
                        }),
                )
            }
            TileAction::Remove => on_media(Button::new(("send-card-remove", id)), cx)
                .icon(IconName::X)
                .tooltip(tr(keys::BOX_REMOVE))
                .accessibility_label(tr_with(
                    keys::Q_CHAT_REMOVE_ATTACHMENT,
                    &[("name", &attachment.file_name)],
                ))
                .on_click({
                    let item = item.clone();
                    cx.listener(move |this, _, _, cx| {
                        this.act_on_tile(&item, |this, index| this.remove_attachment(index, cx))
                    })
                })
                .into_any_element(),
        }
    }

    /// Where the file `item` was drawn for is in the open tray now, if it
    /// still is there: another window of the chat may have removed or added
    /// a file between the frame and the click, which shifts every index.
    /// Then the click is dropped: the same file attached twice has two
    /// entries with one path, and a search by path could act on the other.
    fn tray_index_of(&self, item: &TrayItem) -> Option<usize> {
        if self.send_slot().as_ref() != Some(&item.slot) {
            return None;
        }
        still_at(self.pending_attachments(), item.index, &item.path).then_some(item.index)
    }

    /// Run a tile's action on the file it was drawn for; nothing happens
    /// when that file has left the tray.
    fn act_on_tile(&mut self, item: &TrayItem, act: impl FnOnce(&mut Self, usize)) {
        if let Some(index) = self.tray_index_of(item) {
            act(self, index);
        }
    }

    fn edit_send_card_item(
        &mut self,
        item: &TrayItem,
        kind: EditKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tray_index_of(item) else {
            return;
        };
        match kind {
            EditKind::Photo => self.open_photo_editor(index, cx),
            EditKind::Video => self.open_tray_video_editor(index, window, cx),
        }
    }

    #[inline(never)]
    pub(super) fn send_card_files(
        &self,
        layout: &TrayLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if layout.files.is_empty() {
            return None;
        }
        let rows: Vec<AnyElement> = layout
            .files
            .iter()
            .filter_map(|index| self.send_card_file_row(*index, cx))
            .collect();
        Some(
            div()
                .id("send-card-files")
                .flex()
                .flex_col()
                .gap_1()
                .w_full()
                .max_h(px(4. * (FILE_THUMB + 8.)))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element(),
        )
    }

    fn send_card_file_row(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let attachment = self.pending_attachments().get(index)?;
        let item = self.tray_item(index)?;
        // An image sent as a file still previews as the picture; one the
        // card can't read shows the thumbnail made for sending.
        let picture = attachment
            .extras
            .file_thumbnail
            .as_ref()
            .map(|thumb| thumb.path.clone())
            .or_else(|| {
                (quill::composer::direct_media_kind_for(&attachment.path)
                    == Some(AttachmentKind::Photo))
                .then(|| attachment.path.clone())
            });
        let reading = self
            .composer_sends
            .reads
            .thumbnails
            .is_reading(&attachment.path)
            || self.composer_sends.reads.media.is_reading(&attachment.path);
        let meta = self.send_card_file_meta(attachment);
        let theme = cx.theme();
        let (accent_bg, accent_fg, muted) = (
            theme.primary.opacity(0.12),
            theme.primary,
            theme.muted_foreground,
        );
        Some(
            div()
                .id(("send-card-file", index as u64))
                .flex()
                .items_center()
                .gap_3()
                .p_1()
                .child(
                    div()
                        .flex_none()
                        .size(px(FILE_THUMB))
                        .rounded_md()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(accent_bg)
                        .map(|this| match picture {
                            Some(path) => {
                                this.child(img(path).size_full().object_fit(ObjectFit::Cover))
                            }
                            None => this.child(
                                Icon::new(IconName::File)
                                    .size(px(20.))
                                    .text_color(accent_fg),
                            ),
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .truncate()
                                .child(attachment.file_name.clone()),
                        )
                        .child(div().text_xs().text_color(muted).truncate().child(meta)),
                )
                .when(reading, |row| {
                    row.child(Spinner::new().xsmall().color(muted))
                })
                .child(
                    Button::new(("send-card-file-remove", index as u64))
                        .icon(IconName::X)
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::BOX_REMOVE))
                        .accessibility_label(tr_with(
                            keys::Q_CHAT_REMOVE_ATTACHMENT,
                            &[("name", &attachment.file_name)],
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.act_on_tile(&item, |this, index| this.remove_attachment(index, cx))
                        })),
                )
                .into_any_element(),
        )
    }

    /// "PDF · 1.2 MB" (tdesktop's file row: extension and size).
    fn send_card_file_meta(&self, attachment: &ComposerAttachment) -> String {
        let size = self
            .composer_sends
            .reads
            .file_sizes
            .done(&attachment.path)
            .copied()
            .flatten();
        let extension = attachment
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_uppercase);
        let size = size
            .and_then(|bytes| i64::try_from(bytes).ok())
            .map(quill::telegram::envelope::format_size_text);
        [extension, size]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// A mosaic rounds only its outer corners, as the sent album does.
#[derive(Clone, Copy)]
struct TileCorners {
    tl: bool,
    tr: bool,
    br: bool,
    bl: bool,
}

impl TileCorners {
    fn of(tile: &AlbumRect, (width, height): (i32, i32)) -> Self {
        let (left, top) = (tile.x == 0, tile.y == 0);
        let right = tile.x + tile.width >= width;
        let bottom = tile.y + tile.height >= height;
        Self {
            tl: left && top,
            tr: right && top,
            br: right && bottom,
            bl: left && bottom,
        }
    }

    fn round<E: Styled>(self, element: E, full: bool) -> E {
        if full {
            return element.rounded_full();
        }
        let radius = |outer: bool| px(if outer { TILE_RADIUS } else { INNER_RADIUS });
        element
            .rounded_tl(radius(self.tl))
            .rounded_tr(radius(self.tr))
            .rounded_br(radius(self.br))
            .rounded_bl(radius(self.bl))
    }
}

/// A control over a picture: white on the pill, whatever the theme.
fn on_media(button: Button, cx: &App) -> Button {
    let style = ButtonCustomVariant::new(cx)
        .color(transparent_black())
        .foreground(white())
        .hover(white().opacity(0.2))
        .active(white().opacity(0.3));
    button.custom(style).xsmall().rounded_full()
}

/// A control that shows a choice: [`on_media`] while it is off, dark on
/// the white of [`chosen_mark`] while it is on (a hidden spoiler, a chosen
/// cover).
fn on_media_state(button: Button, chosen: bool, cx: &App) -> Button {
    if !chosen {
        return on_media(button, cx);
    }
    let style = ButtonCustomVariant::new(cx)
        .color(transparent_black())
        .foreground(black())
        .hover(black().opacity(0.15))
        .active(black().opacity(0.25));
    button.custom(style).xsmall().rounded_full()
}

/// The white disc under a control whose choice is on.
fn chosen_mark(chosen: bool, control: impl IntoElement) -> AnyElement {
    div()
        .rounded_full()
        .when(chosen, |mark| mark.bg(white()))
        .child(control)
        .into_any_element()
}

/// A hidden spoiler: darkened under tdesktop's drifting specks, the way
/// the chat will show it.
fn spoiler_veil(corners: TileCorners, round: bool) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .map(|this| corners.round(this, round))
        .bg(black().opacity(0.45))
        .child(
            crate::ui::anim_layer::painter(
                crate::ui::spoiler_fx::specks_fps(),
                |bounds, window| {
                    crate::ui::spoiler_fx::paint_media_specks(bounds, window);
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .into_any_element()
}

impl EditKind {
    fn label(self) -> &'static str {
        match self {
            EditKind::Photo => tr(keys::Q_MISC_EDIT_PHOTO),
            EditKind::Video => tr(keys::Q_OPS_EDIT_VIDEO),
        }
    }
}

fn spoiler_label(hidden: bool) -> &'static str {
    if hidden {
        tr(keys::Q_CHAT_REMOVE_SPOILER)
    } else {
        tr(keys::Q_CHAT_HIDE_WITH_SPOILER)
    }
}

/// Everything a narrow tile does, in one menu.
fn tile_menu(
    mut menu: PopupMenu,
    item: TrayItem,
    actions: &[TileAction],
    owner: WeakEntity<QuillApp>,
) -> PopupMenu {
    for action in actions {
        let owner = owner.clone();
        let item = item.clone();
        menu = match *action {
            TileAction::Edit(kind) => menu.item(
                PopupMenuItem::new(kind.label())
                    .icon(IconName::Pencil)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.edit_send_card_item(&item, kind, window, cx)
                        });
                    }),
            ),
            TileAction::Spoiler { hidden } => menu.item(
                PopupMenuItem::new(spoiler_label(hidden))
                    .icon(if hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    })
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.act_on_tile(&item, |this, index| {
                                this.toggle_attachment_spoiler(index, cx)
                            })
                        });
                    }),
            ),
            TileAction::Cover { chosen } => cover_items(menu, item, chosen, owner),
            TileAction::Remove => menu.separator().item(
                PopupMenuItem::new(tr(keys::BOX_REMOVE))
                    .icon(IconName::X)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.act_on_tile(&item, |this, index| this.remove_attachment(index, cx))
                        });
                    }),
            ),
        };
    }
    menu
}

/// "Edit Cover", and "Clear Cover" for a video that has one.
fn cover_items(
    menu: PopupMenu,
    item: TrayItem,
    chosen: bool,
    owner: WeakEntity<QuillApp>,
) -> PopupMenu {
    let edit = owner.clone();
    let edit_item = item.clone();
    let menu = menu.item(
        PopupMenuItem::new(tr(keys::Q_OPS_EDIT_COVER))
            .icon(IconName::Image)
            .on_click(move |_, _, cx| {
                let _ = edit.update(cx, |this, cx| {
                    this.act_on_tile(&edit_item, |this, index| {
                        this.pick_attachment_cover(index, cx)
                    })
                });
            }),
    );
    if !chosen {
        return menu;
    }
    menu.item(
        PopupMenuItem::new(tr(keys::Q_OPS_CLEAR_COVER))
            .icon(IconName::X)
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.act_on_tile(&item, |this, index| this.clear_attachment_cover(index, cx))
                });
            }),
    )
}

/// Whether the tray still holds the file at `index` that a tile was drawn
/// for.
fn still_at(tray: &[ComposerAttachment], index: usize, path: &std::path::Path) -> bool {
    tray.get(index)
        .is_some_and(|attachment| attachment.path == path)
}

#[cfg(test)]
mod still_at_tests {
    use super::still_at;
    use quill::composer::{AttachmentKind, ComposerAttachment};
    use std::path::Path;

    fn tray(names: &[&str]) -> Vec<ComposerAttachment> {
        let dir = quill::scratch::temp_work_dir("quill-tile-tests");
        std::fs::create_dir_all(&dir).expect("a folder");
        names
            .iter()
            .filter_map(|name| {
                let path = dir.join(name);
                std::fs::write(&path, b"x").ok()?;
                ComposerAttachment::pick(&path, AttachmentKind::Document)
            })
            .collect()
    }

    fn path_of(tray: &[ComposerAttachment], index: usize) -> std::path::PathBuf {
        tray.get(index).map(|a| a.path.clone()).unwrap_or_default()
    }

    #[test]
    fn a_shifted_tray_gives_up_instead_of_acting_on_the_twin_copy() {
        // The same file attached twice; tile 1 was drawn for the second copy.
        let mut tray = tray(&["a.txt", "b.txt"]);
        let (a, b) = (path_of(&tray, 0), path_of(&tray, 1));
        let twin = tray.get(1).cloned().expect("a file");
        tray.push(twin);
        assert!(still_at(&tray, 1, &b) && still_at(&tray, 2, &b));
        // The other window removed the first file: a tile drawn for the
        // third entry finds nothing at its index, and the one drawn for
        // the removed file finds another file there.
        tray.remove(0);
        assert!(!still_at(&tray, 2, &b));
        assert!(!still_at(&tray, 0, &a));
        assert!(still_at(&tray, 1, Path::new(&b)));
    }
}

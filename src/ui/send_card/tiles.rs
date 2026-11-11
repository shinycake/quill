//! The send card's previews: each photo and video message drawn the way it
//! will be sent (tdesktop's album preview in the send box), with its
//! controls on a dark pill that reads over any picture, and the files as
//! rows with a thumbnail, name and size.
#![deny(clippy::indexing_slicing)]

use super::layout::{FULL_CONTROLS_WIDTH, TrayLayout, message_tiles};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
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

/// What a tile's ⋯ menu offers, decided once per render.
#[derive(Clone, Copy)]
struct TileMenu {
    index: usize,
    /// "Edit" and "Remove" are in the menu, not on the pill (a narrow tile).
    narrow: bool,
    edit: Option<EditKind>,
    spoiler: Option<bool>,
    cover: Option<bool>,
}

#[derive(Clone, Copy)]
enum EditKind {
    Photo,
    Video,
}

impl QuillApp {
    #[inline(never)]
    pub(super) fn send_card_media(
        &self,
        layout: &TrayLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if layout.media.is_empty() {
            return None;
        }
        let max_height = layout.max_height();
        let mut strip = div()
            .id("send-card-media")
            .flex()
            .items_end()
            .gap_2()
            .overflow_x_scroll();
        for message in &layout.media {
            let sizes: Vec<(i32, i32)> = message
                .iter()
                .map(|index| self.send_card_tile_size(*index))
                .collect();
            let tiles = message_tiles(&sizes, max_height);
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
            strip = strip.child(block);
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
            _ => self.tray_photo_size(attachment.photo_file()),
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

    /// The tile's controls on one dark pill: Edit, ⋯ and Remove, or only ⋯
    /// on a tile too narrow for three.
    #[inline(never)]
    fn send_card_tile_controls(
        &self,
        index: usize,
        attachment: &ComposerAttachment,
        width: i32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu = self.send_card_tile_menu(index, attachment, width);
        let rendering = attachment.extras.photo_render().is_some();
        let more = menu.has_items().then(|| {
            let owner = cx.entity().downgrade();
            on_media(Button::new(("send-card-more", index as u64)), cx)
                .icon(IconName::Ellipsis)
                .tooltip(tr(keys::Q_CHAT_MORE))
                .dropdown_menu(move |popup, _, _| tile_menu(popup, menu, owner.clone()))
        });
        let edit = menu.edit.filter(|_| !menu.narrow).map(|kind| {
            on_media(Button::new(("send-card-edit", index as u64)), cx)
                .map(|button| {
                    // Its edit is still being saved.
                    if rendering {
                        button
                            .icon(Spinner::new().color(white()))
                            .debug_selector(move || format!("composer-attach-rendering-{index}"))
                    } else {
                        button.icon(IconName::Pencil)
                    }
                })
                .tooltip(match kind {
                    EditKind::Photo => tr(keys::Q_CHAT_EDIT),
                    EditKind::Video => tr(keys::Q_OPS_EDIT_VIDEO),
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.edit_send_card_item(index, kind, window, cx)
                }))
        });
        let remove = (!menu.narrow).then(|| {
            on_media(Button::new(("send-card-remove", index as u64)), cx)
                .icon(IconName::X)
                .tooltip(tr(keys::BOX_REMOVE))
                .accessibility_label(tr_with(
                    keys::Q_CHAT_REMOVE_ATTACHMENT,
                    &[("name", &attachment.file_name)],
                ))
                .on_click(cx.listener(move |this, _, _, cx| this.remove_attachment(index, cx)))
        });
        div()
            .absolute()
            .top_1()
            .right_1()
            .flex()
            .items_center()
            .p(px(2.))
            .rounded_full()
            .bg(black().opacity(0.55))
            .children(edit)
            .children(more)
            .children(remove)
            .into_any_element()
    }

    fn send_card_tile_menu(
        &self,
        index: usize,
        attachment: &ComposerAttachment,
        width: i32,
    ) -> TileMenu {
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
        let spoiler = (media && !self.tray_paid_price_set()).then_some(attachment.spoiler);
        // A video that goes out as a GIF has no cover.
        let cover = (attachment.kind == AttachmentKind::Video
            && edit.is_some()
            && !attachment.sends_as_animation(
                self.pending_attachments().len(),
                self.composer_group_media_effective(),
            )
            && self.session().is_some_and(|session| {
                session
                    .open_chat_id()
                    .is_some_and(|chat| session_allows_cover(session, chat))
            }))
        .then_some(attachment.extras.cover.is_some());
        TileMenu {
            index,
            narrow: width < FULL_CONTROLS_WIDTH,
            edit,
            spoiler,
            cover,
        }
    }

    fn edit_send_card_item(
        &mut self,
        index: usize,
        kind: EditKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                .max_h(px(4. * (FILE_THUMB + 8.)))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element(),
        )
    }

    fn send_card_file_row(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let attachment = self.pending_attachments().get(index)?;
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
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.remove_attachment(index, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    /// "PDF · 1.2 MB" (tdesktop's file row: extension and size).
    fn send_card_file_meta(&self, attachment: &ComposerAttachment) -> String {
        let size = *self
            .composer_sends
            .reads
            .file_sizes
            .borrow_mut()
            .entry(attachment.path.clone())
            .or_insert_with(|| {
                std::fs::metadata(&attachment.path)
                    .ok()
                    .map(|meta| meta.len())
            });
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

impl TileMenu {
    fn has_items(&self) -> bool {
        self.narrow || self.spoiler.is_some() || self.cover.is_some()
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

fn tile_menu(mut menu: PopupMenu, tile: TileMenu, owner: WeakEntity<QuillApp>) -> PopupMenu {
    let index = tile.index;
    if tile.narrow
        && let Some(kind) = tile.edit
    {
        let owner = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(match kind {
                EditKind::Photo => tr(keys::Q_CHAT_EDIT),
                EditKind::Video => tr(keys::Q_OPS_EDIT_VIDEO),
            })
            .icon(IconName::Pencil)
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.edit_send_card_item(index, kind, window, cx)
                });
            }),
        );
    }
    if let Some(spoiler) = tile.spoiler {
        let owner = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(if spoiler {
                tr(keys::Q_CHAT_REMOVE_SPOILER)
            } else {
                tr(keys::Q_CHAT_HIDE_WITH_SPOILER)
            })
            .icon(if spoiler {
                IconName::Eye
            } else {
                IconName::EyeOff
            })
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |this, cx| this.toggle_attachment_spoiler(index, cx));
            }),
        );
    }
    if let Some(has_cover) = tile.cover {
        let edit = owner.clone();
        menu = menu.item(
            PopupMenuItem::new(tr(keys::Q_OPS_EDIT_COVER))
                .icon(IconName::Image)
                .on_click(move |_, _, cx| {
                    let _ = edit.update(cx, |this, cx| this.pick_attachment_cover(index, cx));
                }),
        );
        if has_cover {
            let clear = owner.clone();
            menu = menu.item(PopupMenuItem::new(tr(keys::Q_OPS_CLEAR_COVER)).on_click(
                move |_, _, cx| {
                    let _ = clear.update(cx, |this, cx| this.clear_attachment_cover(index, cx));
                },
            ));
        }
    }
    if tile.narrow {
        menu = menu.separator().item(
            PopupMenuItem::new(tr(keys::BOX_REMOVE))
                .icon(IconName::X)
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.remove_attachment(index, cx));
                }),
        );
    }
    menu
}

//! The Media, Photos, Videos and GIFs tabs as a thumbnail grid under month
//! headers (tdesktop `Info::Media` with `Overview::Layout::{Photo, Video,
//! Gif}`): the minithumbnail blur shows at once, the downloaded thumbnail
//! replaces it, videos carry their length and GIFs a "GIF" mark.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::mini_image::mini_image;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::message_a11y::format_duration_text;
use quill::state::{SharedMediaItem, SharedMediaTab, SharedMediaTile, TileBadge, month_sections};
use std::path::PathBuf;

/// Tile side (three per row in the panel).
const TILE: f32 = 96.;

impl QuillApp {
    /// The `Ready` body of a photo, video or GIF tab.
    pub(super) fn shared_media_tile_grid(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(state) = session.media.shared_media.tab_at(tab) else {
            return div().into_any_element();
        };
        let roots = self.media_display_roots();
        let sections = month_sections(&state.items);
        let mut body = div()
            .id(("shared-media-items", tab.index() as u64))
            .flex_1()
            .overflow_y_scroll()
            .pb_2();
        if let Some(bar) = self.shared_media_date_bar(tab, cx) {
            body = body.child(bar);
        }
        for section in &sections {
            let items = state.items.get(section.start..section.end).unwrap_or(&[]);
            let mut grid = div().flex().flex_wrap().gap_1().px_2();
            for item in items {
                grid = grid.child(self.shared_media_tile_view(item, &roots, cx));
            }
            body = body
                .child(
                    div()
                        .id(("shared-media-month", section.start as u64))
                        .role(gpui_kit::Role::Heading)
                        .aria_label(section.month.title())
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(section.month.title()),
                )
                .child(grid);
        }
        if state.can_load_more() {
            body = body.child(self.shared_media_load_more(tab, cx));
        }
        body.into_any_element()
    }

    fn shared_media_tile_view(
        &self,
        item: &SharedMediaItem,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message_id = item.message_id;
        let action = item.action;
        let tile = item.meta.tile();
        let files = self.session().map(|session| &session.media.files);
        let path = tile.and_then(|tile| {
            tile.thumbs.iter().find_map(|id| {
                let file = files.and_then(|files| files.get(&id.0))?;
                sandboxed_display_path(file.usable_path()?, roots)
            })
        });
        let base = div()
            .id(("shared-media-tile", message_id.0 as u64))
            .relative()
            .size(px(TILE))
            .flex_none()
            .rounded_lg()
            .overflow_hidden()
            .bg(cx.theme().secondary)
            .role(gpui_kit::Role::Button)
            .aria_label(tile_label(item, tile))
            .tab_index(0)
            .cursor_pointer();
        let picture = match (path, tile) {
            (Some(path), tile) => Some(
                img(super::image_budget::sized_media(
                    &path,
                    (px(TILE), px(TILE)),
                    tile.map(|t| (t.width, t.height))
                        .filter(|(w, h)| *w > 0 && *h > 0),
                    super::image_budget::Fit::Cover,
                ))
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            ),
            (None, Some(tile)) => {
                tile.mini
                    .as_ref()
                    .filter(|mini| !mini.data.is_empty())
                    .map(|mini| {
                        img(mini_image(mini))
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .into_any_element()
                    })
            }
            (None, None) => None,
        };
        let badge = tile.and_then(|tile| badge_text(tile.badge));
        base.children(picture)
            .when_some(badge, |this, text| {
                this.child(
                    div()
                        .absolute()
                        .left_1()
                        .bottom_1()
                        .px_1()
                        .rounded_md()
                        .bg(gpui_kit::black().opacity(0.55))
                        .text_xs()
                        .text_color(gpui_kit::white())
                        .child(text),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.activate_shared_media_item(action, message_id, cx);
            }))
            .context_menu({
                let owner = cx.entity().downgrade();
                move |menu, _, _| {
                    let owner = owner.clone();
                    menu.item(PopupMenuItem::new(tr(keys::Q_OPS_GO_TO_MESSAGE)).on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.jump_to_shared_media_item_ui(message_id, cx);
                            });
                        },
                    ))
                }
            })
            .into_any_element()
    }
}

fn badge_text(badge: TileBadge) -> Option<String> {
    match badge {
        TileBadge::None => None,
        TileBadge::Duration(secs) => Some(format_duration_text(i64::from(secs))),
        TileBadge::Gif => Some(tr(keys::Q_OPS_GIF).to_string()),
    }
}

/// The accessible name of a tile: its kind and, for a video, its length.
fn tile_label(item: &SharedMediaItem, tile: Option<&SharedMediaTile>) -> String {
    match tile.and_then(|tile| badge_text(tile.badge)) {
        Some(badge) => format!("{}, {badge}", item.label),
        None => item.label.clone(),
    }
}

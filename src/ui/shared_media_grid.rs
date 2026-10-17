//! The Media, Photos, Videos and GIFs tabs as a thumbnail grid under month
//! headers (tdesktop `Info::Media` with `Overview::Layout::{Photo, Video,
//! Gif}`): the minithumbnail blur shows at once, the downloaded thumbnail
//! replaces it, and videos carry their length. The grid is a virtual list of
//! rows (month header, row of tiles, Load more), so only the tiles on screen
//! are built and ask for their pictures.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::mini_image::mini_image;
use super::shared_media_grid_state::PageKey;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::gpui::Size as ItemSize;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::message_a11y::format_duration_text;
use quill::state::{
    GRID_GAP, GRID_PAD, GridLayout, GridRow, SharedMediaItem, SharedMediaTab, SharedMediaTile,
    TileBadge, grid_rows, month_sections,
};
use std::path::PathBuf;
use std::rc::Rc;

/// The shared-media panel's width (`shell_layout`).
const PANEL_WIDTH: f32 = super::shell_layout::SHARED_MEDIA_PANEL_WIDTH;

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
        let layout = GridLayout::for_width(PANEL_WIDTH);
        let rows = Rc::new(grid_rows(
            &month_sections(&state.items),
            layout.per_row,
            state.can_load_more(),
        ));
        let sizes: Rc<Vec<ItemSize<Pixels>>> = Rc::new(
            rows.iter()
                .map(|row| ItemSize::new(px(0.), px(row.height(&layout))))
                .collect(),
        );
        let mut list = v_virtual_list(
            cx.entity(),
            ("shared-media-items", tab.index() as u64),
            sizes,
            move |this: &mut QuillApp, range, _window, cx| {
                let roots = this.media_display_roots();
                range
                    .filter_map(|ix| rows.get(ix).copied().map(|row| (ix, row)))
                    .map(|(ix, row)| this.shared_media_grid_row(tab, ix, row, &layout, &roots, cx))
                    .collect::<Vec<_>>()
            },
        )
        .flex_1()
        .min_h_0()
        .w_full();
        let key = PageKey {
            chat: session.media.shared_media.chat_id.map_or(0, |chat| chat.0),
            anchor_day: state.anchor_day,
            newest: state.items.first().map_or(0, |item| item.message_id.0),
        };
        if let Some(handle) = self.shared_media_grid.scroll(tab, key) {
            list = list.track_scroll(handle);
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .children(self.shared_media_date_bar(tab, cx))
            .child(list)
            .into_any_element()
    }

    fn shared_media_grid_row(
        &self,
        tab: SharedMediaTab,
        ix: usize,
        row: GridRow,
        layout: &GridLayout,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            GridRow::Header(month) => div()
                .id(("shared-media-month", ix as u64))
                .role(gpui_kit::Role::Heading)
                .aria_label(month.title())
                .h(px(row.height(layout)))
                .flex()
                .items_end()
                .px_3()
                .pb_1()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(month.title())
                .into_any_element(),
            GridRow::Tiles { start, end } => {
                let items = self
                    .session()
                    .and_then(|session| session.media.shared_media.tab_at(tab))
                    .and_then(|state| state.items.get(start..end))
                    .unwrap_or(&[]);
                div()
                    .id(("shared-media-tiles", ix as u64))
                    .flex()
                    .gap(px(GRID_GAP))
                    .px(px(GRID_PAD))
                    .h(px(row.height(layout)))
                    .children(
                        items
                            .iter()
                            .map(|item| self.shared_media_tile_view(item, layout.tile, roots, cx)),
                    )
                    .into_any_element()
            }
            GridRow::LoadMore => div()
                .h(px(row.height(layout)))
                .flex()
                .items_center()
                .child(self.shared_media_load_more(tab, cx))
                .w_full()
                .into_any_element(),
        }
    }

    fn shared_media_tile_view(
        &self,
        item: &SharedMediaItem,
        side: f32,
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
            .debug_selector(move || format!("shared-media-tile-{}", message_id.0))
            .relative()
            .size(px(side))
            .flex_none()
            .overflow_hidden()
            .bg(cx.theme().secondary)
            .role(gpui_kit::Role::Button)
            .aria_label(tile_label(item, tile))
            .tab_index(0)
            .cursor_pointer();
        // The blur lies under the picture, so a tile that scrolled back in
        // shows it while the picture decodes.
        let blur = tile
            .and_then(|tile| tile.mini.as_ref())
            .filter(|mini| !mini.data.is_empty())
            .map(|mini| {
                img(mini_image(mini))
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            });
        let picture = path.map(|path| {
            img(super::image_budget::sized_media(
                &path,
                (px(side), px(side)),
                tile.map(|t| (t.width, t.height))
                    .filter(|(w, h)| *w > 0 && *h > 0),
                super::image_budget::Fit::Cover,
            ))
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element()
        });
        let badge = tile.and_then(|tile| badge_text(tile.badge));
        base.children(blur)
            .children(picture)
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
    }
}

/// The accessible name of a tile: its kind and, for a video, its length.
fn tile_label(item: &SharedMediaItem, tile: Option<&SharedMediaTile>) -> String {
    match tile.and_then(|tile| badge_text(tile.badge)) {
        Some(badge) => format!("{}, {badge}", item.label),
        None => item.label.clone(),
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "shared_media_grid_tests.rs"]
mod tests;

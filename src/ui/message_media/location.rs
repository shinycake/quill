//! Location and venue rows with their map tile.

use super::*;

/// Size the static map tile draws at (the request asks for the same
/// 16:9 box at 2x).
const MAP_TILE_WIDTH: f32 = 256.0;
const MAP_TILE_HEIGHT: f32 = 144.0;

/// The downloaded `getMapThumbnailFile` tile with a pin at its centre;
/// a click opens the place in the browser's map.
fn map_tile(
    id: (&'static str, u64),
    path: PathBuf,
    url: String,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let (width, height) = (px(MAP_TILE_WIDTH), px(MAP_TILE_HEIGHT));
    div()
        .id(id)
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded_md()
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label("Open location in Maps")
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_message_url(&url, cx);
        }))
        .child(
            img(crate::ui::image_budget::sized_media(
                &path,
                (width, height),
                Some((MAP_TILE_WIDTH as i32 * 2, MAP_TILE_HEIGHT as i32 * 2)),
                crate::ui::image_budget::Fit::Cover,
            ))
            .size_full()
            .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(14.))
                        .rounded_full()
                        .bg(accent())
                        .border_2()
                        .border_color(gpui_kit::white())
                        .shadow_md(),
                ),
        )
        .into_any_element()
}

/// Phase 4.3: `messageLocation` / `messageLiveLocation` row. A static map
/// placeholder chip (no live tiles): pin glyph, coordinate line, live
/// status when the message is a live location, and a tappable "Open map"
/// link. The link opens an OpenStreetMap URL through
/// `platform::open_external_url` (https only, scheme-gated — no `geo:`
/// or `tel:` schemes). A running share's countdown is recomputed on every
/// draw and the window redraws as it changes (`live_location_tick`).
pub(in crate::ui) fn location_row(
    row_id: u64,
    location: &quill::telegram::envelope::GeoLocation,
    live: Option<&quill::telegram::envelope::LiveLocationState>,
    // Set when this is your own running live location: the message that
    // the "Stop sharing" button ends.
    stop: Option<(ChatId, MessageId)>,
    tile: Option<PathBuf>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = location.open_street_map_url();
    let tile = tile.map(|path| map_tile(("location-map", row_id), path, url.clone(), cx));
    let header = if live.is_some() {
        "📍 Live location"
    } else {
        "📍 Location"
    };
    let mut body = div()
        .id(("location-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .children(tile)
        .child(
            div()
                .text_sm()
                .font_medium()
                .text_color(text_primary())
                .child(header),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_primary())
                .child(location.coords_label()),
        );
    if let Some(live) = live {
        body = body.child(
            div()
                .text_xs()
                .text_color(accent())
                .child(live.status_label()),
        );
    } else if location.accuracy_m > 0 {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(format!("accuracy ±{} m", location.accuracy_m)),
        );
    }
    if let Some((chat_id, message_id)) = stop {
        body = body.child(
            div().flex().child(
                Button::new(format!("live-location-stop-{row_id}"))
                    .label("Stop sharing")
                    .small()
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stop_live_location(chat_id, message_id, cx);
                    })),
            ),
        );
    }
    body.child(
        div()
            .id(("location-open-map", row_id))
            .text_sm()
            .text_color(accent())
            .role(gpui_kit::Role::Button)
            .aria_label("Open location in Maps")
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_message_url(&url, cx);
            }))
            .child("🗺 Open map"),
    )
    .into_any_element()
}

/// Phase 4.3: `messageVenue` row — venue title, address, optional provider
/// subtitle, and a tappable "Open map" link on the venue's coordinates
/// (same OpenStreetMap handling as `location_row`). The provider `id` /
/// `type` are not kept in the model (see `VenueContent`).
pub(in crate::ui) fn venue_row(
    row_id: u64,
    venue: &quill::telegram::envelope::VenueContent,
    tile: Option<PathBuf>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let url = venue.location.open_street_map_url();
    let tile = tile.map(|path| map_tile(("venue-map", row_id), path, url.clone(), cx));
    let title = if venue.title.is_empty() {
        "Venue".to_string()
    } else {
        venue.title.clone()
    };
    let mut body = div()
        .id(("venue-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .children(tile)
        .child(div().text_sm().font_medium().child(format!("📍 {title}")));
    if !venue.address.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(text_primary())
                .child(venue.address.clone()),
        );
    }
    let subtitle = if venue.provider.is_empty() {
        venue.location.coords_label()
    } else {
        format!("{} · via {}", venue.location.coords_label(), venue.provider)
    };
    body.child(div().text_xs().text_color(text_muted()).child(subtitle))
        .child(
            div()
                .id(("venue-open-map", row_id))
                .text_sm()
                .text_color(accent())
                .role(gpui_kit::Role::Button)
                .aria_label("Open venue in Maps")
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_message_url(&url, cx);
                }))
                .child("🗺 Open map"),
        )
        .into_any_element()
}

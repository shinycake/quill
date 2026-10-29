//! The picker window (tdesktop `pickLocationWindow`, 364 wide): the title
//! with its search, the map, "Send This Location" with the address, and
//! "Or choose a venue" with the venues, their loading and empty states and
//! the provider line. The map collapses as the list scrolls, as in
//! tdesktop.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::pressable::PressableDiv;
use super::super::shell::{DialogKind, QuillShell};
use super::map::{MAP_COLLAPSED, MAP_HEIGHT};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::location_picker::{AddressLine, HereState, Venue, VenueList};

/// tdesktop `pickLocationWindow` width and the list rows' height
/// (`pickLocationRowHeight`).
const WINDOW_WIDTH: f32 = 364.;
const BODY_HEIGHT: f32 = 600.;
const ROW_HEIGHT: f32 = 52.;
const ICON_SIZE: f32 = 42.;
/// tdesktop `pickLocationLoading` area.
const LOADING_HEIGHT: f32 = 56.;

/// A venue's circle: a hue picked by its category, like tdesktop's
/// `EmptyUserpic::ColorIndex(UniqueRowIdFromString(icon))`.
fn venue_color(venue_type: &str) -> Hsla {
    let hue = (gpui_kit::hash(&SharedString::from(venue_type.to_string())) % 12) as f32 * 30.;
    oklch(0.64, 0.13, hue)
}

impl QuillApp {
    pub(super) fn location_search_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::DLG_FILTER)));
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change)
                    || this.location_ui.search.as_ref().map(|s| s.entity_id())
                        != Some(state.entity_id())
                {
                    return;
                }
                let text = state.read(cx).value().to_string();
                let _ = this.in_window_of(window, cx, |this, _, cx| {
                    this.location_search_changed(Some(text), cx);
                });
            },
        )
        .detach();
        input.update(cx, |input, cx| input.focus(window, cx));
        self.location_ui.search = Some(input);
        self.location_search_changed(Some(String::new()), cx);
    }

    fn location_search_close(&mut self, cx: &mut Context<Self>) {
        self.location_ui.search = None;
        self.location_search_changed(None, cx);
    }

    fn location_title(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(search) = self.location_ui.search.as_ref() {
            return h_flex()
                .w_full()
                .pl_4()
                .pr_8()
                .gap_1()
                .debug_selector(|| "location-search".to_string())
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.location_search_close(cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    Input::new(search)
                        .small()
                        .prefix(Icon::new(IconName::Search).small())
                        .aria_label(tr(keys::DLG_FILTER)),
                )
                .child(
                    div()
                        .debug_selector(|| "location-search-close".to_string())
                        .child(
                            Button::new("location-search-close")
                                .icon(IconName::Close)
                                .small()
                                .ghost()
                                .accessibility_label(tr(keys::Q_LOCATION_CLOSE_SEARCH))
                                .tooltip(tr(keys::Q_LOCATION_CLOSE_SEARCH))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.location_search_close(cx)),
                                ),
                        ),
                )
                .into_any_element();
        }
        let can_search = self
            .session()
            .and_then(|session| session.location.picker())
            .is_some_and(|picker| picker.can_search());
        h_flex()
            .w_full()
            .pl_4()
            .pr_8()
            .justify_between()
            .items_center()
            .child(crate::ui::shell::dialog_title(tr(keys::MAPS_POINT)))
            .when(can_search, |row| {
                row.child(
                    div()
                        .debug_selector(|| "location-search-open".to_string())
                        .child(
                            Button::new("location-search-open")
                                .icon(IconName::Search)
                                .small()
                                .ghost()
                                .accessibility_label(tr(keys::DLG_FILTER))
                                .tooltip(tr(keys::DLG_FILTER))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.location_search_open(window, cx)
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn location_row_icon(&self, color: Hsla, image: Option<std::path::PathBuf>) -> AnyElement {
        let circle = div()
            .size(px(ICON_SIZE))
            .flex_shrink_0()
            .rounded_full()
            .bg(color)
            .flex()
            .items_center()
            .justify_center();
        match image {
            Some(path) => circle.child(img(path).size(px(ICON_SIZE - 12.))),
            None => circle.child(Icon::new(IconName::MapPin).text_color(gpui_kit::white())),
        }
        .into_any_element()
    }

    /// "Send This Location" (tdesktop `MakeChooseLocationButton`).
    fn location_send_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let address = self
            .session()
            .and_then(|session| session.location.picker())
            .map(|picker| match picker.address_line() {
                AddressLine::Text(text) => Some(SharedString::from(text.to_string())),
                AddressLine::Loading => Some(tr(keys::CONTACTS_LOADING).into()),
                AddressLine::Hidden => None,
            })
            .unwrap_or_default();
        let accent = cx.theme().primary;
        div()
            .id("location-send")
            .debug_selector(|| "location-send".to_string())
            .role(Role::Button)
            .aria_label(tr(keys::MAPS_POINT_SEND))
            .tab_index(0)
            .h(px(ROW_HEIGHT))
            .px(px(18.))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(|this, _, _, cx| this.send_picked_location(cx)))
            .child(
                div()
                    .size(px(ICON_SIZE))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::MapPin).text_color(cx.theme().primary_foreground)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .truncate()
                            .child(tr(keys::MAPS_POINT_SEND)),
                    )
                    .children(address.map(|address| {
                        div()
                            .id("location-address")
                            .debug_selector(|| "location-address".to_string())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(address)
                    })),
            )
            .into_any_element()
    }

    fn location_venue_row(&self, venue: &Venue, cx: &mut Context<Self>) -> AnyElement {
        let icon = self.session().and_then(|session| {
            let file_id = session.location.icon_file(&venue.venue_type)?;
            let path = session.media.files.get(&file_id.0)?.usable_path()?;
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
        let selector = format!("location-venue-{}", venue.id);
        let picked = venue.clone();
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .role(Role::Button)
            .aria_label(SharedString::from(venue.title.clone()))
            .tab_index(0)
            .h(px(ROW_HEIGHT))
            .px(px(18.))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| this.send_picked_venue(picked.clone(), cx)))
            .child(self.location_row_icon(venue_color(&venue.venue_type), icon))
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .truncate()
                            .child(venue.title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(venue.address.clone()),
                    ),
            )
            .into_any_element()
    }

    fn location_note(&self, id: &'static str, text: SharedString, cx: &App) -> AnyElement {
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .px(px(18.))
            .py_4()
            .text_sm()
            .text_center()
            .text_color(cx.theme().muted_foreground)
            .child(text)
            .into_any_element()
    }

    fn location_venues(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(picker) = self.session().and_then(|session| session.location.picker()) else {
            return Vec::new();
        };
        match picker.venues() {
            VenueList::Loading => vec![
                div()
                    .id("location-venues-loading")
                    .debug_selector(|| "location-venues-loading".to_string())
                    .h(px(LOADING_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new())
                    .into_any_element(),
            ],
            VenueList::NothingFound { .. } => vec![self.location_note(
                "location-venues-none",
                tr(keys::MAPS_NO_PLACES).into(),
                cx,
            )],
            VenueList::WaitingForLocation => {
                let why = match picker.here() {
                    HereState::Denied => Some(tr(keys::Q_LOCATION_DENIED)),
                    HereState::Unavailable => Some(tr(keys::Q_LOCATION_UNAVAILABLE)),
                    HereState::Locating | HereState::Found => None,
                };
                let mut out = Vec::new();
                if let Some(why) = why {
                    out.push(self.location_note("location-here-state", why.into(), cx));
                }
                out.push(self.location_note(
                    "location-venues-waiting",
                    tr(keys::MAPS_CHOOSE_TO_SEARCH).into(),
                    cx,
                ));
                out
            }
            VenueList::Found(venues) => {
                let mut out: Vec<AnyElement> = venues
                    .iter()
                    .map(|venue| self.location_venue_row(venue, cx))
                    .collect();
                out.push(
                    div()
                        .debug_selector(|| "location-venues-source".to_string())
                        .py_2()
                        .text_xs()
                        .text_center()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::MAPS_VENUES_SOURCE))
                        .into_any_element(),
                );
                out
            }
        }
    }

    fn location_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let searching = self
            .session()
            .and_then(|session| session.location.picker())
            .is_some_and(|picker| picker.search_shown());
        let scrolled = -f32::from(self.location_ui.list_scroll.offset().y);
        let map_height = (MAP_HEIGHT - scrolled.max(0.)).max(MAP_COLLAPSED);
        let mut list = v_flex()
            .id("location-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.location_ui.list_scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()));
        if !searching {
            list = list
                .child(self.location_send_row(cx))
                .child(div().h(px(1.)).bg(cx.theme().border))
                .child(
                    div()
                        .px(px(18.))
                        .pt_3()
                        .pb_1()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().primary)
                        .child(tr(keys::MAPS_OR_CHOOSE)),
                );
        }
        list = list.children(self.location_venues(cx));
        v_flex()
            .id("location-picker")
            .debug_selector(|| "location-picker".to_string())
            .w_full()
            .h(px(BODY_HEIGHT))
            .when(!searching, |body| {
                body.child(self.location_map(map_height, cx))
            })
            .child(list)
            .into_any_element()
    }

    pub(super) fn build_location_picker_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LocationPicker, |this, _, cx| {
                this.close_location_picker(cx);
            });
        let body_of = app.downgrade();
        app.update(cx, |this, cx| {
            let searching = this.location_ui.search.is_some();
            let title = this.location_title(cx);
            dialog
                .w(px(WINDOW_WIDTH))
                .px_0()
                .pb_0()
                .overlay(true)
                .keyboard(!searching)
                .title(title)
                .content(move |content, _, cx| {
                    let body = body_of.update(cx, |this, cx| this.location_body(cx)).ok();
                    content.px_0().children(body)
                })
                .on_close(on_close)
        })
    }
}

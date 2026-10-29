//! The picker's map, drawn natively from TDLib's static map images
//! (`getMapThumbnailFile`) in place of tdesktop's Mapbox web view: the
//! images the picker holds, placed for the current view (an old one shifts
//! and scales with a pan or zoom until the new one arrives), a fixed pin in
//! the middle that lifts while the map moves, zoom and "my location"
//! buttons, and "Places in this area" once the map rests somewhere new.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::state::Drag;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::location_picker::{MapSize, MapView};
use quill::state::Session;

/// tdesktop `pickLocationMapHeight` / `pickLocationCollapsedHeight`.
pub(super) const MAP_HEIGHT: f32 = 220.;
pub(super) const MAP_COLLAPSED: f32 = 92.;
const PIN_LIFT: f32 = 24.;
const KEY_PAN: f64 = 80.;
const SCROLL_PER_STEP: f32 = 120.;

/// The Mapbox marker tdesktop shows (27×41, tip at 13.5,35).
fn pin_path(origin: Point<Pixels>) -> Option<Path<Pixels>> {
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut path = PathBuilder::fill();
    path.move_to(at(27., 13.5));
    path.cubic_bezier_to(at(14.75, 34.5), at(27., 19.07), at(20.25, 27.));
    path.cubic_bezier_to(at(12.25, 34.5), at(14.02, 35.5), at(12.98, 35.5));
    path.cubic_bezier_to(at(0., 13.5), at(6.75, 27.), at(0., 19.22));
    path.cubic_bezier_to(at(13.5, 0.), at(0., 6.04), at(6.04, 0.));
    path.cubic_bezier_to(at(27., 13.5), at(20.96, 0.), at(27., 6.04));
    path.close();
    path.build().ok()
}

fn paint_pin(center: Point<Pixels>, lift: f32, color: Hsla, dot: Hsla, window: &mut Window) {
    let shadow = Bounds::centered_at(point(center.x, center.y - px(0.2)), size(px(21.), px(10.5)));
    window.paint_quad(fill(shadow, gpui_kit::black().opacity(0.18)).corner_radii(px(5.25)));
    let origin = point(center.x - px(13.5), center.y - px(35. + lift));
    if let Some(path) = pin_path(origin) {
        window.paint_path(path, color);
    }
    let dot_center = point(origin.x + px(13.5), origin.y + px(13.5));
    window.paint_quad(
        fill(Bounds::centered_at(dot_center, size(px(11.), px(11.))), dot).corner_radii(px(5.5)),
    );
}

fn tile_images(
    session: &Session,
    roots: &[std::path::PathBuf],
    visible: MapSize,
) -> Vec<(
    std::path::PathBuf,
    quill::location_picker::map_view::TilePlacement,
)> {
    let Some(picker) = session.location.picker() else {
        return Vec::new();
    };
    let view = picker.view();
    picker
        .tiles()
        .into_iter()
        .filter_map(|(key, file_id)| {
            let placement = key.placement(&view, visible);
            if !placement.visible_in(visible) {
                return None;
            }
            let path = session.media.files.get(&file_id.0)?.usable_path()?;
            let path = quill::local_path::sandboxed_display_path(path, roots)?;
            Some((path, placement))
        })
        .collect()
}

impl QuillApp {
    fn location_view(&self) -> Option<MapView> {
        self.session()?
            .location
            .picker()
            .map(|picker| picker.view())
    }

    fn location_zoom(&mut self, steps: i32, cx: &mut Context<Self>) {
        let Some(view) = self.location_view() else {
            return;
        };
        let zoomed = view.zoomed(steps);
        if zoomed != view {
            self.location_map_moved(zoomed, cx);
        }
    }

    fn location_pan(&mut self, dx: f64, dy: f64, cx: &mut Context<Self>) {
        if let Some(view) = self.location_view() {
            self.location_map_moved(view.panned(dx, dy), cx);
        }
    }

    fn location_scrolled(&mut self, delta: ScrollDelta, cx: &mut Context<Self>) {
        let steps = match delta {
            ScrollDelta::Lines(lines) => lines.y,
            ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / SCROLL_PER_STEP,
        };
        self.location_zoom_by(steps, cx);
    }

    fn location_zoom_by(&mut self, steps: f32, cx: &mut Context<Self>) {
        if !steps.is_finite() {
            return;
        }
        let carry = self.location_ui.zoom_carry + steps;
        let whole = carry.trunc();
        self.location_ui.zoom_carry = carry - whole;
        if whole != 0. {
            self.location_ui.zoom_carry = 0.;
            self.location_zoom(whole as i32, cx);
        }
    }

    fn location_map_released(&mut self, cx: &mut Context<Self>) {
        if self.location_ui.drag.take().is_none() {
            return;
        }
        if self
            .session()
            .and_then(|session| session.location.picker())
            .is_some_and(|picker| picker.is_moving())
            && let Some(view) = self.location_view()
        {
            self.location_map_moved(view, cx);
        }
    }

    fn location_map_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        match key {
            "left" => self.location_pan(KEY_PAN, 0., cx),
            "right" => self.location_pan(-KEY_PAN, 0., cx),
            "up" => self.location_pan(0., KEY_PAN, cx),
            "down" => self.location_pan(0., -KEY_PAN, cx),
            "+" | "=" => self.location_zoom(1, cx),
            "-" => self.location_zoom(-1, cx),
            _ => return false,
        }
        true
    }

    pub(super) fn location_map(&self, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(picker) = session.location.picker() else {
            return div().into_any_element();
        };
        let measured = picker.map_size();
        let lifted = self.location_ui.drag.is_some() || picker.is_moving();
        let view = picker.view();
        let places = picker.places_in_area_shown();
        let tiles = measured
            .map(|size| {
                let visible = MapSize::new(size.width as f32, height, f32::from(size.scale));
                tile_images(session, &self.media_display_roots(), visible)
            })
            .unwrap_or_default();
        let theme = cx.theme();
        let (pin, dot, ground) = (theme.primary, theme.primary_foreground, theme.muted);
        let app = cx.entity().downgrade();
        let gestures = canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, cx| {
                let want = MapSize::new(
                    f32::from(bounds.size.width),
                    MAP_HEIGHT,
                    window.scale_factor(),
                );
                if measured != Some(want) {
                    let app = app.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = app.update(cx, |this, cx| this.location_map_measured(want, cx));
                    });
                }
                let lift = if lifted { PIN_LIFT } else { 0. };
                paint_pin(bounds.center(), lift, pin, dot, window);
                {
                    let (app, hitbox) = (app.clone(), hitbox.clone());
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble
                            || event.button != MouseButton::Left
                            || !hitbox.is_hovered(window)
                        {
                            return;
                        }
                        let position = event.position;
                        let _ = app.update(cx, |this, _| {
                            this.location_ui.drag = Some(Drag { last: position });
                        });
                    });
                }
                {
                    let app = app.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture
                            || event.pressed_button != Some(MouseButton::Left)
                        {
                            return;
                        }
                        let position = event.position;
                        let _ = app.update(cx, |this, cx| {
                            let Some(drag) = this.location_ui.drag else {
                                return;
                            };
                            let dx = f64::from(f32::from(position.x - drag.last.x));
                            let dy = f64::from(f32::from(position.y - drag.last.y));
                            this.location_ui.drag = Some(Drag { last: position });
                            if dx != 0. || dy != 0. {
                                this.location_pan(dx, dy, cx);
                            }
                        });
                    });
                }
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = app.update(cx, |this, cx| this.location_map_released(cx));
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full();

        let mut map = div()
            .id("location-map")
            .debug_selector(|| "location-map".to_string())
            .role(Role::Group)
            .aria_label(tr(keys::Q_LOCATION_MAP))
            .relative()
            .w_full()
            .h(px(height))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(ground)
            .cursor(if lifted {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                this.location_scrolled(event.delta, cx);
                cx.stop_propagation();
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                this.location_zoom_by((1. + event.delta).max(0.01).log2(), cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.location_map_key(&event.keystroke.key, cx) {
                    cx.stop_propagation();
                }
            }));
        if let Some(focus) = self.location_ui.map_focus.as_ref() {
            map = map.track_focus(focus);
        }
        for (index, (path, at)) in tiles.into_iter().enumerate() {
            map = map.child(
                img(path)
                    .id(("location-tile", index))
                    .absolute()
                    .left(px(at.left))
                    .top(px(at.top))
                    .w(px(at.width))
                    .h(px(at.height))
                    .object_fit(ObjectFit::Fill),
            );
        }
        map = map.child(gestures);
        map = map.child(
            v_flex()
                .absolute()
                .top_2()
                .right_2()
                .gap_1()
                .occlude()
                .child(
                    div()
                        .debug_selector(|| "location-zoom-in".to_string())
                        .child(
                            Button::new("location-zoom-in")
                                .icon(IconName::Plus)
                                .small()
                                .secondary()
                                .accessibility_label(tr(keys::MEDIA_ZOOM_IN))
                                .tooltip(tr(keys::MEDIA_ZOOM_IN))
                                .disabled(!view.can_zoom_in())
                                .on_click(cx.listener(|this, _, _, cx| this.location_zoom(1, cx))),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| "location-zoom-out".to_string())
                        .child(
                            Button::new("location-zoom-out")
                                .icon(IconName::Minus)
                                .small()
                                .secondary()
                                .accessibility_label(tr(keys::MEDIA_ZOOM_OUT))
                                .tooltip(tr(keys::MEDIA_ZOOM_OUT))
                                .disabled(!view.can_zoom_out())
                                .on_click(cx.listener(|this, _, _, cx| this.location_zoom(-1, cx))),
                        ),
                ),
        );
        map = map.child(
            div()
                .absolute()
                .bottom_2()
                .right_2()
                .occlude()
                .debug_selector(|| "location-my-location".to_string())
                .child(
                    Button::new("location-my-location")
                        .icon(IconName::LocateFixed)
                        .small()
                        .secondary()
                        .accessibility_label(tr(keys::Q_LOCATION_MY_LOCATION))
                        .tooltip(tr(keys::Q_LOCATION_MY_LOCATION))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.location_picker_locate_again(cx)),
                        ),
                ),
        );
        if places && !lifted {
            map = map.child(
                h_flex()
                    .absolute()
                    .top_2()
                    .left_0()
                    .right_0()
                    .justify_center()
                    .child(
                        div()
                            .occlude()
                            .debug_selector(|| "location-places-in-area".to_string())
                            .rounded_full()
                            .shadow_md()
                            .child(
                                Button::new("location-places-in-area")
                                    .label(tr(keys::MAPS_PLACES_IN_AREA))
                                    .small()
                                    .secondary()
                                    .rounded(ButtonRounded::Large)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.location_places_in_area(cx)
                                    })),
                            ),
                    ),
            );
        }
        map.into_any_element()
    }
}

//! The picker's map as a Web Mercator viewport, and the static tiles
//! (`getMapThumbnailFile`) it is drawn from.
//!
//! TDLib snaps a thumbnail's centre to a whole world pixel at its zoom
//! (`FileManager::get_map_thumbnail_file_id`), so a [`TileKey`] is that
//! pixel plus the size: the same key always names the same image.

use super::geo::GeoPoint;
use std::f64::consts::PI;

/// `getMapThumbnailFile` accepts zoom 13 to 20.
pub const MIN_ZOOM: u8 = 13;
pub const MAX_ZOOM: u8 = 20;
/// The zoom the picker flies to for a known place (tdesktop
/// `LocationPicker.startZoom`).
pub const START_ZOOM: u8 = 14;
/// `getMapThumbnailFile` width and height bounds, before the scale.
const TILE_EDGE: std::ops::RangeInclusive<u32> = 16..=1024;

/// The world's edge in pixels at `zoom` (256 per tile, as TDLib counts).
fn world_size(zoom: u8) -> f64 {
    256. * f64::from(1u32 << zoom.min(MAX_ZOOM))
}

/// The shortest horizontal offset in a world `world` pixels wide that
/// repeats across the antimeridian.
fn nearest_wrapped(dx: f64, world: f64) -> f64 {
    if dx > world / 2. {
        dx - world
    } else if dx < -world / 2. {
        dx + world
    } else {
        dx
    }
}

/// A position in world pixels at some zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint {
    pub x: f64,
    pub y: f64,
}

pub fn project(point: GeoPoint, zoom: u8) -> WorldPoint {
    let size = world_size(zoom);
    let sin = (point.latitude() * PI / 180.).sin();
    WorldPoint {
        x: (point.longitude() + 180.) / 360. * size,
        y: (0.5 - ((1. + sin) / (1. - sin)).ln() / (4. * PI)) * size,
    }
}

pub fn unproject(world: WorldPoint, zoom: u8) -> GeoPoint {
    let size = world_size(zoom);
    let longitude = world.x / size * 360. - 180.;
    let n = PI - 2. * PI * world.y / size;
    let latitude = (n.sinh()).atan() * 180. / PI;
    GeoPoint::new(latitude, longitude).unwrap_or(GeoPoint::ORIGIN)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapView {
    center: GeoPoint,
    zoom: u8,
}

impl MapView {
    pub fn new(center: GeoPoint, zoom: u8) -> Self {
        Self {
            center,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }

    pub fn center(&self) -> GeoPoint {
        self.center
    }

    pub fn zoom(&self) -> u8 {
        self.zoom
    }

    /// The view after the map was dragged by `(dx, dy)` logical pixels
    /// (the content follows the pointer, so the centre moves the other way).
    pub fn panned(&self, dx: f64, dy: f64) -> Self {
        let size = world_size(self.zoom);
        let world = project(self.center, self.zoom);
        let moved = WorldPoint {
            x: (world.x - dx).rem_euclid(size),
            y: (world.y - dy).clamp(0., size),
        };
        Self {
            center: unproject(moved, self.zoom),
            zoom: self.zoom,
        }
    }

    /// One or more zoom steps in (`steps > 0`) or out, about the pin.
    pub fn zoomed(&self, steps: i32) -> Self {
        let zoom = (i32::from(self.zoom) + steps).clamp(i32::from(MIN_ZOOM), i32::from(MAX_ZOOM));
        Self::new(self.center, u8::try_from(zoom).unwrap_or(MIN_ZOOM))
    }

    pub fn can_zoom_in(&self) -> bool {
        self.zoom < MAX_ZOOM
    }

    pub fn can_zoom_out(&self) -> bool {
        self.zoom > MIN_ZOOM
    }
}

/// The map area in logical pixels and the display scale tiles are asked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapSize {
    pub width: u32,
    pub height: u32,
    /// `getMapThumbnailFile.scale`, 1 to 3.
    pub scale: u8,
}

impl MapSize {
    pub fn new(width: f32, height: f32, scale: f32) -> Self {
        let edge = |value: f32| {
            let value = if value.is_finite() { value.round() } else { 0. };
            (value.max(0.) as u32).clamp(*TILE_EDGE.start(), *TILE_EDGE.end())
        };
        let scale = if scale.is_finite() { scale.ceil() } else { 1. };
        Self {
            width: edge(width),
            height: edge(height),
            scale: scale.clamp(1., 3.) as u8,
        }
    }
}

/// One static map image: TDLib's world pixel at `zoom` plus the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub zoom: u8,
    pub x: i64,
    pub y: i64,
    pub size: MapSize,
}

impl TileKey {
    /// The image centred on what `view` shows (TDLib truncates the
    /// centre to a whole pixel).
    pub fn for_view(view: &MapView, size: MapSize) -> Self {
        let world = project(view.center, view.zoom);
        let last = world_size(view.zoom) - 1.;
        Self {
            zoom: view.zoom,
            x: world.x.clamp(0., last) as i64,
            y: world.y.clamp(0., last) as i64,
            size,
        }
    }

    /// The location to send in the request: the middle of the key's pixel,
    /// so TDLib's truncation lands on it again.
    pub fn request_center(&self) -> GeoPoint {
        unproject(
            WorldPoint {
                x: self.x as f64 + 0.5,
                y: self.y as f64 + 0.5,
            },
            self.zoom,
        )
    }

    /// Where this image sits in a map of `size` showing `view`: its
    /// top-left corner and its width / height, in logical pixels. A tile
    /// of another zoom is scaled by the zoom difference.
    pub fn placement(&self, view: &MapView, size: MapSize) -> TilePlacement {
        let factor = 2f64.powi(i32::from(view.zoom) - i32::from(self.zoom));
        let center = project(view.center, view.zoom);
        let world = world_size(view.zoom);
        let dx = nearest_wrapped(self.x as f64 * factor - center.x, world);
        let dy = self.y as f64 * factor - center.y;
        let width = f64::from(self.size.width) * factor;
        let height = f64::from(self.size.height) * factor;
        TilePlacement {
            left: (f64::from(size.width) / 2. + dx - width / 2.) as f32,
            top: (f64::from(size.height) / 2. + dy - height / 2.) as f32,
            width: width as f32,
            height: height as f32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilePlacement {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl TilePlacement {
    pub fn visible_in(&self, size: MapSize) -> bool {
        self.left < size.width as f32
            && self.top < size.height as f32
            && self.left + self.width > 0.
            && self.top + self.height > 0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).expect("finite")
    }

    const SIZE: MapSize = MapSize {
        width: 364,
        height: 220,
        scale: 2,
    };

    #[test]
    fn projection_round_trips() {
        let p = point(32.0853, 34.7818);
        for zoom in MIN_ZOOM..=MAX_ZOOM {
            let back = unproject(project(p, zoom), zoom);
            assert!(back.same_place(&p), "zoom {zoom}: {back:?}");
        }
    }

    #[test]
    fn projection_matches_tdlib_pixels() {
        let world = project(GeoPoint::ORIGIN, 13);
        assert_eq!(world.x as i64, 256 * 8192 / 2);
        assert_eq!(world.y as i64, 256 * 8192 / 2);
    }

    #[test]
    fn dragging_right_moves_the_centre_west() {
        let view = MapView::new(point(10., 20.), 15);
        let moved = view.panned(100., 0.);
        assert!(moved.center().longitude() < 20.);
        assert!((moved.center().latitude() - 10.).abs() < 1e-6);
        let back = moved.panned(-100., 0.);
        assert!(back.center().same_place(&view.center()));
    }

    #[test]
    fn panning_wraps_and_clamps() {
        let view = MapView::new(point(0., 179.999), 13);
        assert!(view.panned(-5_000., 0.).center().longitude() < 0.);
        let north = MapView::new(point(85., 0.), 13).panned(0., 1e9);
        assert!(north.center().latitude() > 85.);
    }

    #[test]
    fn zoom_stays_in_tdlib_range() {
        let view = MapView::new(GeoPoint::ORIGIN, 3);
        assert_eq!(view.zoom(), MIN_ZOOM);
        assert!(!view.can_zoom_out());
        assert_eq!(view.zoomed(100).zoom(), MAX_ZOOM);
        assert_eq!(view.zoomed(1).zoom(), 14);
    }

    #[test]
    fn map_size_clamps_to_request_bounds() {
        let size = MapSize::new(2000., 4., 2.0);
        assert_eq!((size.width, size.height, size.scale), (1024, 16, 2));
        assert_eq!(MapSize::new(f32::NAN, 100., 1.5).scale, 2);
        assert_eq!(MapSize::new(100., 100., 9.).scale, 3);
    }

    #[test]
    fn a_tile_for_the_view_is_centred() {
        let view = MapView::new(point(51.5, -0.12), 15);
        let key = TileKey::for_view(&view, SIZE);
        let place = key.placement(&view, SIZE);
        assert!((place.left).abs() <= 1.);
        assert!((place.top).abs() <= 1.);
        assert_eq!(place.width, 364.);
        assert_eq!(
            TileKey::for_view(&MapView::new(key.request_center(), 15), SIZE),
            key
        );
    }

    #[test]
    fn a_panned_view_shifts_the_old_tile() {
        let view = MapView::new(point(51.5, -0.12), 15);
        let key = TileKey::for_view(&view, SIZE);
        let moved = view.panned(50., -20.);
        let place = key.placement(&moved, SIZE);
        assert!((place.left - 50.).abs() <= 1.5);
        assert!((place.top + 20.).abs() <= 1.5);
        assert!(place.visible_in(SIZE));
        assert!(
            !key.placement(&view.panned(2000., 0.), SIZE)
                .visible_in(SIZE)
        );
    }

    #[test]
    fn a_tile_from_the_previous_zoom_is_scaled() {
        let view = MapView::new(point(51.5, -0.12), 15);
        let key = TileKey::for_view(&view, SIZE);
        let place = key.placement(&view.zoomed(1), SIZE);
        assert!((place.width - 728.).abs() < 0.01);
        assert!((place.left + 182.).abs() <= 2.);
    }
}

//! Reading TwidgetCore's JSON (display lists and canvas batches,
//! `twlite::to_json`) into tiny-skia's shapes, paints and transforms.
use serde_json::Value;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Path, PathBuilder, Point, RadialGradient,
    Rect, Shader, SpreadMode, Stroke, StrokeDash, SweepGradient, Transform,
};

pub fn num(value: &Value, key: &str) -> f32 {
    value.get(key).and_then(Value::as_f64).map_or(0.0, |v| v as f32)
}

pub fn int(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

pub fn flag(value: &Value, key: &str) -> bool {
    int(value, key) != 0
}

pub fn floats(value: &Value, key: &str) -> Vec<f32> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
        .unwrap_or_default()
}

/// The `n`th of `values`, 0 past the end.
pub fn at(values: &[f32], n: usize) -> f32 {
    values.get(n).copied().unwrap_or(0.0)
}

/// An ARGB number as a color.
pub fn argb(value: u64) -> Color {
    let byte = |shift: u32| ((value >> shift) & 0xff) as u8;
    Color::from_rgba8(byte(16), byte(8), byte(0), byte(24))
}

pub fn color(value: &Value, key: &str) -> Color {
    argb(value.get(key).and_then(Value::as_u64).unwrap_or(0))
}

/// "r": [x, y, w, h]; `None` without one.
pub fn rect(value: &Value, key: &str) -> Option<Rect> {
    let r = floats(value, key);
    if r.len() < 4 {
        return None;
    }
    Rect::from_xywh(at(&r, 0), at(&r, 1), at(&r, 2), at(&r, 3))
}

/// "m": [a, b, c, d, tx, ty], SVG order.
pub fn matrix(value: &Value, key: &str) -> Transform {
    let m = floats(value, key);
    if m.len() < 6 {
        return Transform::identity();
    }
    Transform::from_row(at(&m, 0), at(&m, 1), at(&m, 2), at(&m, 3), at(&m, 4), at(&m, 5))
}

/// "s": offset, ARGB pairs.
pub fn stops(value: &Value, key: &str) -> Vec<GradientStop> {
    let Some(items) = value.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[offset, color]| {
            GradientStop::new(offset.as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32, argb(color.as_u64().unwrap_or(0)))
        })
        .collect()
}

/// A path from "o" op codes (0 move, 1 line, 2 quad, 3 cubic, 4 close) and "v" points.
pub fn path(value: &Value) -> Option<Path> {
    let ops = value.get("o").and_then(Value::as_array)?;
    let v = floats(value, "v");
    let mut builder = PathBuilder::new();
    let mut i = 0;
    let mut take = |n: usize| {
        let out: Vec<f32> = (0..n).map(|k| at(&v, i + k)).collect();
        i += n;
        out
    };
    for op in ops {
        match op.as_i64().unwrap_or(-1) {
            0 => {
                let p = take(2);
                builder.move_to(at(&p, 0), at(&p, 1));
            }
            1 => {
                let p = take(2);
                builder.line_to(at(&p, 0), at(&p, 1));
            }
            2 => {
                let p = take(4);
                builder.quad_to(at(&p, 0), at(&p, 1), at(&p, 2), at(&p, 3));
            }
            3 => {
                let p = take(6);
                builder.cubic_to(at(&p, 0), at(&p, 1), at(&p, 2), at(&p, 3), at(&p, 4), at(&p, 5));
            }
            4 => builder.close(),
            _ => return None,
        }
    }
    builder.finish()
}

pub fn fill_rule(value: &Value) -> FillRule {
    if flag(value, "e") { FillRule::EvenOdd } else { FillRule::Winding }
}

/// A rectangle with CSS corner radii ("q": tlx, tly, trx, try, brx, bry, blx, bly).
pub fn rounded(rect: Rect, radii: &[f32]) -> Option<Path> {
    if radii.iter().all(|r| *r <= 0.0) {
        return Some(PathBuilder::from_rect(rect));
    }
    let (x, y, w, h) = (rect.x(), rect.y(), rect.width(), rect.height());
    // CSS scales all radii down together when adjacent ones overlap.
    let mut f: f32 = 1.0;
    let pairs = [(at(radii, 0) + at(radii, 2), w), (at(radii, 6) + at(radii, 4), w), (at(radii, 1) + at(radii, 7), h), (at(radii, 3) + at(radii, 5), h)];
    for (sum, side) in pairs {
        if sum > side && sum > 0.0 {
            f = f.min(side / sum);
        }
    }
    let r = |n: usize| at(radii, n).max(0.0) * f;
    // The cubic of a quarter ellipse.
    const K: f32 = 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(x + r(0), y);
    p.line_to(x + w - r(2), y);
    p.cubic_to(x + w - r(2) * (1.0 - K), y, x + w, y + r(3) * (1.0 - K), x + w, y + r(3));
    p.line_to(x + w, y + h - r(5));
    p.cubic_to(x + w, y + h - r(5) * (1.0 - K), x + w - r(4) * (1.0 - K), y + h, x + w - r(4), y + h);
    p.line_to(x + r(6), y + h);
    p.cubic_to(x + r(6) * (1.0 - K), y + h, x, y + h - r(7) * (1.0 - K), x, y + h - r(7));
    p.line_to(x, y + r(1));
    p.cubic_to(x, y + r(1) * (1.0 - K), x + r(0) * (1.0 - K), y, x + r(0), y);
    p.close();
    p.finish()
}

/// `rect` grown by `by` on every side (shrunk for a negative `by`), radii with it.
pub fn inflate(rect: Rect, radii: &[f32], by: f32) -> Option<(Rect, Vec<f32>)> {
    let grown = Rect::from_ltrb(rect.left() - by, rect.top() - by, rect.right() + by, rect.bottom() + by)?;
    let radii = radii.iter().map(|r| if *r > 0.0 { (r + by).max(0.0) } else { 0.0 }).collect();
    Some((grown, radii))
}

/// A line style: width, "lc" cap, "lj" join, "ml" miter, "d" dashes, "do" offset.
pub fn stroke(value: &Value, width: f32, cap: &str, join: &str, miter: &str, dashes: &str, offset: &str) -> Stroke {
    let dash = floats(value, dashes);
    Stroke {
        width: width.max(0.0),
        miter_limit: num(value, miter).max(1.0),
        line_cap: match int(value, cap) {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match int(value, join) {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        dash: if dash.is_empty() {
            None
        } else {
            let even = if dash.len() % 2 == 1 { [dash.clone(), dash].concat() } else { dash };
            StrokeDash::new(even, num(value, offset))
        },
    }
}

/// A box gradient of the display list: `kind` 6 linear (a = from, to),
/// 7 radial (a = centre, radii), 8 conic (a = centre, angle from the top).
pub fn box_gradient(kind: i64, a: &[f32], stops: Vec<GradientStop>) -> Option<Shader<'static>> {
    let (ax, ay, bx, by) = (at(a, 0), at(a, 1), at(a, 2), at(a, 3));
    match kind {
        6 => LinearGradient::new(Point::from_xy(ax, ay), Point::from_xy(bx, by), stops, SpreadMode::Pad, Transform::identity()),
        7 => {
            let (rx, ry) = (bx.max(0.01), by.max(0.01));
            let squash = Transform::from_translate(ax, ay)
                .pre_scale(1.0, ry / rx)
                .pre_translate(-ax, -ay);
            RadialGradient::new(Point::from_xy(ax, ay), 0.0, Point::from_xy(ax, ay), rx, stops, SpreadMode::Pad, squash)
        }
        8 => SweepGradient::new(
            Point::from_xy(ax, ay),
            0.0,
            360.0,
            stops,
            SpreadMode::Pad,
            Transform::from_rotate_at(bx - 90.0, ax, ay),
        ),
        _ => None,
    }
}

/// An SVG path's gradient: `kind` 1 linear (x1, y1 → x2, y2), 2 radial
/// (centre x1, y1, radius r, focus fx, fy).
pub fn path_gradient(kind: i64, g: &[f32], stops: Vec<GradientStop>) -> Option<Shader<'static>> {
    let (x1, y1, x2, y2, r, fx, fy) = (at(g, 0), at(g, 1), at(g, 2), at(g, 3), at(g, 4), at(g, 5), at(g, 6));
    match kind {
        1 => LinearGradient::new(Point::from_xy(x1, y1), Point::from_xy(x2, y2), stops, SpreadMode::Pad, Transform::identity()),
        2 => RadialGradient::new(Point::from_xy(fx, fy), 0.0, Point::from_xy(x1, y1), r.max(0.01), stops, SpreadMode::Pad, Transform::identity()),
        _ => None,
    }
}

/// The device rectangle `rect` covers under `transform`, in whole pixels.
pub fn device_bounds(rect: Rect, transform: Transform) -> Option<tiny_skia::IntRect> {
    let mut points = [
        Point::from_xy(rect.left(), rect.top()),
        Point::from_xy(rect.right(), rect.top()),
        Point::from_xy(rect.right(), rect.bottom()),
        Point::from_xy(rect.left(), rect.bottom()),
    ];
    transform.map_points(&mut points);
    let bounds = Rect::from_points(&points)?;
    tiny_skia::IntRect::from_ltrb(
        bounds.left().floor() as i32,
        bounds.top().floor() as i32,
        bounds.right().ceil() as i32,
        bounds.bottom().ceil() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn colors_are_argb() {
        let c = argb(0x80ff_0000);
        assert_eq!((c.red(), c.green(), c.alpha()), (1.0, 0.0, 128.0 / 255.0));
    }

    #[test]
    fn paths_read_every_op_and_refuse_unknown_ones() {
        let p = json!({"o": [0, 1, 2, 3, 4], "v": [0, 0, 10, 0, 12, 4, 10, 10, 6, 12, 2, 12, 0, 10]});
        let built = path(&p).unwrap();
        assert_eq!(built.bounds().right(), 12.0);
        assert!(path(&json!({"o": [0, 9], "v": [0, 0]})).is_none());
    }

    #[test]
    fn overlapping_radii_shrink_together() {
        let rect = Rect::from_xywh(0.0, 0.0, 20.0, 10.0).unwrap();
        let built = rounded(rect, &[20.0; 8]).unwrap();
        assert_eq!(built.bounds(), rect);
    }
}

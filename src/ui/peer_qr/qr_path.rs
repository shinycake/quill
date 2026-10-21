//! The code's modules as one SVG path, rounded like tdesktop's lib_qr:
//! a module's outer corner is round where neither side neighbour is dark,
//! an inner corner between two dark neighbours is filled in, and the three
//! finder patterns are rounded squares. Shared by the box and the saved
//! card image.
#![deny(clippy::indexing_slicing)]

use std::fmt::Write as _;

/// The module grid of an encoded link.
pub(super) struct QrGrid {
    pub(super) size: usize,
    dark: Vec<bool>,
}

const FINDER: usize = 7;
const ROUND: f64 = 0.5;

impl QrGrid {
    /// `link` encoded at error correction M (the disc in the middle
    /// covers under 4 % of the modules).
    pub(super) fn encode(link: &str) -> Option<QrGrid> {
        let code = qrcode::QrCode::with_error_correction_level(link.as_bytes(), qrcode::EcLevel::M)
            .ok()?;
        let size = code.width();
        let dark = code
            .to_colors()
            .into_iter()
            .map(|color| color == qrcode::Color::Dark)
            .collect();
        Some(QrGrid { size, dark })
    }

    #[cfg(test)]
    pub(super) fn from_rows(rows: &[&str]) -> QrGrid {
        QrGrid {
            size: rows.len(),
            dark: rows
                .iter()
                .flat_map(|row| row.chars().map(|c| c == '#'))
                .collect(),
        }
    }

    fn at(&self, x: isize, y: isize) -> bool {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        x < self.size && y < self.size && self.dark.get(y * self.size + x).copied().unwrap_or(false)
    }

    fn in_finder(&self, x: usize, y: usize) -> bool {
        let far = self.size.saturating_sub(FINDER);
        (x < FINDER && y < FINDER) || (x >= far && y < FINDER) || (x < FINDER && y >= far)
    }

    /// The path in module units (the grid is `size` × `size`), every
    /// shape wound the same way so one nonzero fill covers seams.
    pub(super) fn path(&self) -> String {
        let mut d = String::new();
        let far = self.size.saturating_sub(FINDER) as f64;
        for (x, y) in [(0., 0.), (far, 0.), (0., far)] {
            finder(&mut d, x, y);
        }
        for y in 0..self.size {
            for x in 0..self.size {
                if self.in_finder(x, y) {
                    continue;
                }
                let (xi, yi) = (x as isize, y as isize);
                let n = |dx: isize, dy: isize| self.at(xi + dx, yi + dy);
                let (fx, fy) = (x as f64, y as f64);
                if n(0, 0) {
                    let round = |a: bool, b: bool| if a || b { 0. } else { ROUND };
                    module(
                        &mut d,
                        fx,
                        fy,
                        [
                            round(n(0, -1), n(-1, 0)),
                            round(n(0, -1), n(1, 0)),
                            round(n(0, 1), n(1, 0)),
                            round(n(0, 1), n(-1, 0)),
                        ],
                    );
                } else {
                    for (corner, (dx, dy)) in
                        [(-1, -1), (1, -1), (1, 1), (-1, 1)].iter().enumerate()
                    {
                        if n(0, *dy) && n(*dx, 0) && n(*dx, *dy) {
                            fillet(&mut d, fx, fy, corner);
                        }
                    }
                }
            }
        }
        d
    }
}

/// A square with per-corner radii (top left, top right, bottom right,
/// bottom left), clockwise.
fn module(d: &mut String, x: f64, y: f64, r: [f64; 4]) {
    let [tl, tr, br, bl] = r;
    let _ = write!(d, "M{} {}H{}", x + tl, y, x + 1. - tr);
    arc(d, tr, x + 1., y + tr);
    let _ = write!(d, "V{}", y + 1. - br);
    arc(d, br, x + 1. - br, y + 1.);
    let _ = write!(d, "H{}", x + bl);
    arc(d, bl, x, y + 1. - bl);
    let _ = write!(d, "V{}", y + tl);
    arc(d, tl, x + tl, y);
    d.push('Z');
}

fn arc(d: &mut String, r: f64, x: f64, y: f64) {
    if r > 0. {
        let _ = write!(d, "A{r} {r} 0 0 1 {x} {y}");
    }
}

/// The concave piece that joins two dark neighbours across a light
/// module's `corner` (0 top left, then clockwise).
fn fillet(d: &mut String, x: f64, y: f64, corner: usize) {
    let r = ROUND;
    let (cx, cy, sx, sy) = match corner {
        0 => (x, y, 1., 1.),
        1 => (x + 1., y, -1., 1.),
        2 => (x + 1., y + 1., -1., -1.),
        _ => (x, y + 1., 1., -1.),
    };
    // Clockwise like the modules: the first edge runs along x for the
    // top-left and bottom-right corners, along y for the other two.
    let (ax, ay, bx, by) = if sx * sy > 0. {
        (cx + sx * r, cy, cx, cy + sy * r)
    } else {
        (cx, cy + sy * r, cx + sx * r, cy)
    };
    let _ = write!(d, "M{cx} {cy}L{ax} {ay}A{r} {r} 0 0 0 {bx} {by}Z");
}

/// tdesktop's finder: a 7×7 rounded ring and a 3×3 rounded center.
fn finder(d: &mut String, x: f64, y: f64) {
    rounded(d, x, y, 7., 2., true);
    rounded(d, x + 1., y + 1., 5., 1.25, false);
    rounded(d, x + 2., y + 2., 3., 1., true);
}

/// A rounded square, clockwise (`true`) or counter-clockwise to cut a
/// hole.
fn rounded(d: &mut String, x: f64, y: f64, side: f64, r: f64, clockwise: bool) {
    let (x2, y2) = (x + side, y + side);
    if clockwise {
        let _ = write!(
            d,
            "M{} {y}H{}A{r} {r} 0 0 1 {x2} {}V{}A{r} {r} 0 0 1 {} {y2}H{}A{r} {r} 0 0 1 {x} {}V{}A{r} {r} 0 0 1 {} {y}Z",
            x + r,
            x2 - r,
            y + r,
            y2 - r,
            x2 - r,
            x + r,
            y2 - r,
            y + r,
            x + r,
        );
    } else {
        let _ = write!(
            d,
            "M{} {y}H{}A{r} {r} 0 0 0 {x} {}V{}A{r} {r} 0 0 0 {} {y2}H{}A{r} {r} 0 0 0 {x2} {}V{}A{r} {r} 0 0 0 {} {y}Z",
            x2 - r,
            x + r,
            y + r,
            y2 - r,
            x + r,
            x2 - r,
            y2 - r,
            y + r,
            x2 - r,
        );
    }
}

/// The Quill mark (assets/icons/tray-template.svg) in a 100 × 100 box:
/// the Q ring, the feather and the feather's notch.
const MARK_RING: &str = "M50 10C27 10 13 26 13 46C13 63 24 75 39 79L44 70C32 67 24 58 24 46C24 31 35 21 50 21C64 21 74 31 74 45C74 52 71 58 66 62L72 70C80 64 85 55 85 45C85 25 70 10 50 10Z";
const MARK_FEATHER: &str =
    "M40 74C52 66 66 63 76 66C84 69 89 77 91 90C82 84 74 82 66 83C57 84 49 81 40 74Z";
const MARK_NOTCH: &str = "M47 75C60 72 72 73 86 85C72 76 60 75 47 75Z";
/// The disc over the code's middle, over the code's side (tdesktop
/// `kLogoRatio`), and the mark's share of the disc.
const LOGO: f64 = 0.175;
const MARK_IN_LOGO: f64 = 0.62;

/// The code in `ink` in module units, with the white Quill mark on an
/// ink disc in the middle (where tdesktop draws its plane).
pub(super) fn qr_body(grid: &QrGrid, ink: u32) -> String {
    let n = grid.size as f64;
    let disc = n * LOGO;
    let mark = disc * MARK_IN_LOGO;
    let at = (n - mark) / 2.;
    format!(
        r##"<path fill="#{ink:06X}" fill-rule="nonzero" d="{d}"/><circle cx="{c}" cy="{c}" r="{r}" fill="#{ink:06X}"/><g transform="translate({at} {at}) scale({s})"><path fill="#FFFFFF" d="{ring}"/><path fill="#FFFFFF" d="{feather}"/><path fill="#{ink:06X}" d="{notch}"/></g>"##,
        d = grid.path(),
        c = n / 2.,
        r = disc / 2.,
        s = mark / 100.,
        ring = MARK_RING,
        feather = MARK_FEATHER,
        notch = MARK_NOTCH,
    )
}

/// The code as a standalone SVG, `side` points square.
pub(super) fn qr_svg(grid: &QrGrid, side: f64, ink: u32) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}" viewBox="0 0 {n} {n}">{body}</svg>"#,
        n = grid.size,
        body = qr_body(grid, ink),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 15×15 grid, light but for `dark` (column, row) modules outside
    /// the finder corners.
    fn grid(dark: &[(usize, usize)]) -> QrGrid {
        let rows: Vec<String> = (0..15)
            .map(|y| {
                (0..15)
                    .map(|x| if dark.contains(&(x, y)) { '#' } else { '.' })
                    .collect()
            })
            .collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        QrGrid::from_rows(&rows)
    }

    #[test]
    fn lone_modules_are_round_and_neighbours_join() {
        assert_eq!(grid(&[(7, 7)]).path().matches("A0.5 0.5 0 0 1").count(), 4);
        // Side by side, each keeps the two corners away from the other.
        assert_eq!(
            grid(&[(7, 7), (8, 7)])
                .path()
                .matches("A0.5 0.5 0 0 1")
                .count(),
            4
        );
    }

    #[test]
    fn an_inner_corner_is_filled() {
        assert!(!grid(&[(7, 7), (8, 7)]).path().contains('L'));
        assert_eq!(
            grid(&[(7, 7), (8, 7), (7, 8)]).path().matches('L').count(),
            1
        );
    }

    #[test]
    fn a_link_encodes_into_a_square_grid() {
        let grid = QrGrid::encode("https://t.me/quill").expect("encodes");
        assert!(grid.size >= 21);
        let svg = qr_svg(&grid, 202., 0x112233);
        assert!(svg.contains("fill=\"#112233\""));
    }
}

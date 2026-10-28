//! Golden frames for the vendored ThorVG (third_party/thorvg): a few hand-written
//! stickers (tests/fixtures/stickers/generate.py makes the two big ones) rendered at 128 px against committed reference
//! frames, so a ThorVG update shows every pixel it changes.
//!
//! After a bump, regenerate and look at what changed:
//!   QUILL_GOLDEN_DIFF=<dir> cargo test --no-default-features --test sticker_golden -- --ignored
//! writes the new references, plus old, new and diff images to <dir>.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test code is exempt (docs/contributing/panic-safety.md)"
)]

use quill::sticker_playback::{FitzType, decode_tgs_sized};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const EDGE: usize = 128;
/// Frames decoded per case, evenly spaced; the references are all but the
/// first (frame 0 is often blank or static).
const SAMPLES: usize = 3;
/// A pixel is off when a premultiplied channel moves by more than this.
const CHANNEL_TOLERANCE: u8 = 16;
/// The share of off pixels a frame may have: room for compiler and
/// platform rounding at antialiased edges, not for a changed shape.
const MAX_OFF_SHARE: f64 = 0.005;

struct Case {
    name: &'static str,
    /// Under tests/fixtures/stickers: `.tgs`, or `.json` gzipped here.
    file: &'static str,
    fitz: FitzType,
}

const CASES: &[Case] = &[
    // Track matte over a moving bar, and an animated subtract mask.
    Case {
        name: "matte-mask",
        file: "matte-mask.json",
        fitz: FitzType::None,
    },
    // Skin tone table: fill, keyframed stroke and solid layer recoloured,
    // gradient stops kept.
    Case {
        name: "fitz-face-none",
        file: "fitz-face.json",
        fitz: FitzType::None,
    },
    Case {
        name: "fitz-face-type5",
        file: "fitz-face.json",
        fitz: FitzType::Type5,
    },
    // Trim paths on 48 animated paths.
    Case {
        name: "trim-paths",
        file: "trim-paths.json",
        fitz: FitzType::None,
    },
    // 104 animated paths in layers and a precomp: past the shape budget
    // that made the previous renderer drop layers
    // (docs/decisions/thorvg-sticker-renderer.md).
    Case {
        name: "many-paths",
        file: "many-paths.json",
        fitz: FitzType::None,
    },
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stickers")
}

fn golden(name: &str, index: usize) -> PathBuf {
    fixtures()
        .join("golden")
        .join(format!("{name}-{index}.png"))
}

/// The case's sampled frames after the first, straight-alpha BGRA.
fn render(case: &Case) -> Vec<Vec<u8>> {
    let source = fixtures().join(case.file);
    let tgs = if case.file.ends_with(".json") {
        let path = std::env::temp_dir().join(format!(
            "quill-golden-{}-{}.tgs",
            std::process::id(),
            case.name
        ));
        let mut gzip = flate2::write::GzEncoder::new(
            std::fs::File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        gzip.write_all(&std::fs::read(&source).unwrap()).unwrap();
        gzip.finish().unwrap();
        path
    } else {
        source.clone()
    };
    let decoded = decode_tgs_sized(
        &tgs,
        EDGE,
        SAMPLES,
        f64::INFINITY,
        case.fitz,
        &AtomicBool::new(false),
    );
    if tgs != source {
        let _ = std::fs::remove_file(&tgs);
    }
    let frames = decoded
        .unwrap_or_else(|err| panic!("{}: {err}", case.name))
        .frames;
    assert_eq!(frames.len(), SAMPLES, "{}", case.name);
    frames.into_iter().skip(1).collect()
}

fn premultiplied(bgra: &[u8]) -> Vec<u8> {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u16::from(p[3]);
            let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            [mul(p[0]), mul(p[1]), mul(p[2]), p[3]]
        })
        .collect()
}

/// Off pixels (premultiplied) and a diff image: red where off.
fn compare(expected: &[u8], actual: &[u8]) -> (usize, Vec<u8>) {
    let (expected, actual) = (premultiplied(expected), premultiplied(actual));
    let mut off = 0;
    let mut diff = Vec::with_capacity(actual.len());
    for (e, a) in expected
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
    {
        let moved = e.iter().zip(a).map(|(e, a)| e.abs_diff(*a)).max().unwrap();
        if moved > CHANNEL_TOLERANCE {
            off += 1;
            diff.extend([0, 0, 255, 255]);
        } else {
            diff.extend([a[0] / 3, a[1] / 3, a[2] / 3, 255]);
        }
    }
    (off, diff)
}

#[test]
fn stickers_match_their_golden_frames() {
    let report = std::env::temp_dir().join("quill-sticker-golden");
    let mut failures = Vec::new();
    for case in CASES {
        for (index, actual) in render(case).iter().enumerate() {
            let path = golden(case.name, index);
            let expected = read_png(&path).unwrap_or_else(|err| {
                panic!(
                    "{}: {err}; regenerate (see the top of this file)",
                    path.display()
                )
            });
            let (off, diff) = compare(&expected, actual);
            let share = off as f64 / (EDGE * EDGE) as f64;
            if share > MAX_OFF_SHARE {
                std::fs::create_dir_all(&report).unwrap();
                write_png(
                    &report.join(format!("{}-{index}.actual.png", case.name)),
                    actual,
                );
                write_png(
                    &report.join(format!("{}-{index}.diff.png", case.name)),
                    &diff,
                );
                failures.push(format!(
                    "{}-{index}: {:.2}% of pixels off",
                    case.name,
                    share * 100.0
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}\nactual and diff images in {}",
        failures.join("\n"),
        report.display()
    );
}

#[test]
fn a_skin_tone_changes_the_golden_face() {
    let none = read_png(&golden("fitz-face-none", 0)).unwrap();
    let toned = read_png(&golden("fitz-face-type5", 0)).unwrap();
    let (off, _) = compare(&none, &toned);
    assert!(off > EDGE * EDGE / 10, "only {off} pixels changed");
}

#[test]
#[ignore = "writes tests/fixtures/stickers/golden; run after a ThorVG bump"]
fn regenerate_golden_frames() {
    let review = std::env::var_os("QUILL_GOLDEN_DIFF").map(PathBuf::from);
    if let Some(dir) = &review {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::create_dir_all(fixtures().join("golden")).unwrap();
    for case in CASES {
        for (index, frame) in render(case).iter().enumerate() {
            let path = golden(case.name, index);
            if let Some(dir) = &review
                && let Ok(old) = read_png(&path)
            {
                let (off, diff) = compare(&old, frame);
                println!("{}-{index}: {off} pixels off", case.name);
                write_png(&dir.join(format!("{}-{index}.old.png", case.name)), &old);
                write_png(&dir.join(format!("{}-{index}.new.png", case.name)), frame);
                write_png(&dir.join(format!("{}-{index}.diff.png", case.name)), &diff);
            }
            write_png(&path, frame);
        }
    }
}

// A minimal PNG codec for 128 px RGBA images, so the core crate (no `image`
// dependency) can read and write viewable references.

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend(kind);
    out.extend(data);
    out.extend(crc.sum().to_be_bytes());
}

/// Writes straight-alpha BGRA as an RGBA PNG.
fn write_png(path: &Path, bgra: &[u8]) {
    let mut raw = Vec::with_capacity(bgra.len() + EDGE);
    for row in bgra.as_chunks::<{ EDGE * 4 }>().0 {
        raw.push(0);
        for p in row.as_chunks::<4>().0 {
            raw.extend([p[2], p[1], p[0], p[3]]);
        }
    }
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    zlib.write_all(&raw).unwrap();
    let mut header = Vec::new();
    header.extend((EDGE as u32).to_be_bytes());
    header.extend((EDGE as u32).to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib.finish().unwrap());
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).unwrap();
}

/// Reads an 8-bit RGBA, non-interlaced `EDGE` × `EDGE` PNG as BGRA.
fn read_png(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let mut rest = bytes
        .strip_prefix(b"\x89PNG\r\n\x1a\n")
        .ok_or("not a PNG")?;
    let mut idat = Vec::new();
    while rest.len() >= 12 {
        let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        let kind = &rest[4..8];
        let data = rest.get(8..8 + len).ok_or("truncated PNG")?;
        match kind {
            b"IHDR" => {
                let mut want = Vec::new();
                want.extend((EDGE as u32).to_be_bytes());
                want.extend((EDGE as u32).to_be_bytes());
                want.extend([8, 6, 0, 0, 0]);
                if data != want {
                    return Err("not a 128 px 8-bit RGBA PNG".into());
                }
            }
            b"IDAT" => idat.extend(data),
            _ => {}
        }
        rest = &rest[12 + len..];
    }
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(idat.as_slice())
        .read_to_end(&mut raw)
        .map_err(|err| err.to_string())?;
    let stride = EDGE * 4;
    if raw.len() != EDGE * (stride + 1) {
        return Err("wrong PNG size".into());
    }
    let mut rgba = vec![0u8; EDGE * stride];
    for y in 0..EDGE {
        let filter = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let left = if x >= 4 { rgba[y * stride + x - 4] } else { 0 };
            let up = if y > 0 { rgba[(y - 1) * stride + x] } else { 0 };
            let corner = if x >= 4 && y > 0 {
                rgba[(y - 1) * stride + x - 4]
            } else {
                0
            };
            let predicted = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => {
                    let p = i16::from(left) + i16::from(up) - i16::from(corner);
                    let (pa, pb, pc) = (
                        (p - i16::from(left)).abs(),
                        (p - i16::from(up)).abs(),
                        (p - i16::from(corner)).abs(),
                    );
                    if pa <= pb && pa <= pc {
                        left
                    } else if pb <= pc {
                        up
                    } else {
                        corner
                    }
                }
                _ => return Err(format!("unknown PNG filter {filter}")),
            };
            rgba[y * stride + x] = line[x].wrapping_add(predicted);
        }
    }
    Ok(rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect())
}

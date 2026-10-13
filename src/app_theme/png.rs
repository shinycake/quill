//! One-pixel PNGs: how tdesktop's own themes store a solid background
//! (the bundled `night.tdesktop-theme` holds a 1x1 `background.png`), and
//! how Quill writes one. Larger images are decoded by the UI.

use std::io::{Read, Write};

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

/// A 1x1 RGB PNG of 0xRRGGBB.
pub fn solid_png(rgb: u32) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    let row = [0, (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(&row);
    chunk(&mut out, b"IDAT", &encoder.finish().unwrap_or_default());
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The colour of a 1x1, 8-bit RGB or RGBA PNG; `None` for anything else.
pub fn solid_color(png: &[u8]) -> Option<u32> {
    let mut at: usize = 8;
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut color_type = None;
    let mut idat = Vec::new();
    while let Some(head) = png.get(at..at.checked_add(8)?) {
        let (len, kind) = head.split_at(4);
        let len = u32::from_be_bytes(len.try_into().ok()?) as usize;
        let body = at.checked_add(8)?;
        let data = png.get(body..body.checked_add(len)?)?;
        match kind {
            b"IHDR" => {
                // A truncated header (shorter than width, height, bit
                // depth and colour type) is not a PNG we can read.
                let [w0, w1, w2, w3, h0, h1, h2, h3, depth, color, ..] = *data else {
                    return None;
                };
                if [w0, w1, w2, w3] != [0, 0, 0, 1]
                    || [h0, h1, h2, h3] != [0, 0, 0, 1]
                    || depth != 8
                {
                    return None;
                }
                color_type = Some(color);
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at = body.checked_add(len)?.checked_add(4)?;
    }
    let channels = match color_type? {
        2 => 3,
        6 => 4,
        _ => return None,
    };
    let mut row = Vec::new();
    flate2::read::ZlibDecoder::new(idat.as_slice())
        .take(16)
        .read_to_end(&mut row)
        .ok()?;
    // One pixel: every filter predicts from zeros (no left or upper
    // neighbour), so the bytes are the colour as written.
    let (&filter, px) = row.split_first()?;
    if filter > 4 || px.len() < channels {
        return None;
    }
    let [r, g, b, ..] = *px else { return None };
    Some((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

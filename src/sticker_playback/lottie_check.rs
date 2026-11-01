//! The caps and the real stickers they were set against are in
//! docs/decisions/thorvg-sticker-renderer.md.
#![deny(clippy::indexing_slicing)]

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_ulong};

const MAX_NUMBER: f64 = 1_048_576.0;
const MAX_LAYERS: usize = 8192;
const MAX_MASKS: usize = 256;
const MAX_PRECOMP_DEPTH: usize = 32;
const MAX_GROUP_DEPTH: usize = 64;
const MAX_REPEATS: f64 = 1024.0;
const MAX_GRADIENT_STOPS: u64 = 1024;
const MAX_WORK: u64 = 1 << 22;
const VERTEX_WORK: u64 = 16;
const SHAPE_WORK: u64 = 64;
const OFFSCREEN_WORK: u64 = 1024;
const DASHED_PAINTS: u64 = 16;
const OFFSET_COPIES: u64 = 9;
const MAX_LAYER_KIND: u64 = 15;
const PRECOMP: u64 = 0;
const TEXT: u64 = 5;
const CUSTOM_EFFECT: u64 = 5;
const ANIMATION_SECTIONS: [&str; 3] = ["layers", "assets", "chars"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Malformed {
    Number,
    Key,
    Array,
    Layer,
    Shape,
    Asset,
    TooComplex,
}

pub(super) fn check(doc: &Value) -> Result<(), Malformed> {
    if work(doc)? > MAX_WORK {
        return Err(Malformed::TooComplex);
    }
    Ok(())
}

fn work(doc: &Value) -> Result<u64, Malformed> {
    for section in ANIMATION_SECTIONS {
        if let Some(value) = doc.get(section) {
            check_values(value)?;
        }
    }
    let mut walk = Walk::new(doc)?;
    let root = doc
        .get("layers")
        .and_then(Value::as_array)
        .ok_or(Malformed::Layer)?;
    let cost = walk.layers(root, 0)?;
    // An asset no layer shows is parsed all the same.
    for index in 0..walk.assets.len() {
        walk.asset(index, 0)?;
    }
    Ok(cost.work)
}

/// `serde_json`'s recursion limit bounds the depth.
fn check_values(value: &Value) -> Result<(), Malformed> {
    match value {
        Value::Number(number) => {
            if number.as_f64().is_some_and(|n| n.abs() <= MAX_NUMBER) {
                Ok(())
            } else {
                Err(Malformed::Number)
            }
        }
        Value::Array(items) => {
            if items.iter().any(Value::is_null) {
                return Err(Malformed::Array);
            }
            items.iter().try_for_each(check_values)
        }
        Value::Object(fields) => {
            for (key, field) in fields {
                if key.contains('\0') {
                    return Err(Malformed::Key);
                }
                if key == "k"
                    && let Value::Array(items) = field
                    && !items.iter().all(Value::is_number)
                    && !items.iter().all(Value::is_object)
                {
                    return Err(Malformed::Array);
                }
                check_values(field)?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(()),
    }
}

fn peak(property: &Value) -> f64 {
    let (mut biggest, mut handle) = (0.0, 0.0);
    scan(property, &mut biggest, &mut handle);
    biggest * easing_overshoot(handle)
}

fn easing_overshoot(handle: f64) -> f64 {
    1.0 + 2.0 * handle
}

/// `serde_json`'s recursion limit bounds the depth.
fn scan(value: &Value, biggest: &mut f64, handle: &mut f64) {
    match value {
        Value::Number(number) => {
            *biggest = number.as_f64().map_or(*biggest, |n| biggest.max(n.abs()))
        }
        Value::Array(items) => items.iter().for_each(|item| scan(item, biggest, handle)),
        Value::Object(fields) => {
            for (key, field) in fields {
                match key.as_str() {
                    "k" | "s" | "e" => scan(field, biggest, handle),
                    "i" | "o" => {
                        if let Some(y) = field.get("y") {
                            scan(y, handle, &mut 0.0);
                        }
                    }
                    _ => {}
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::String(_) => {}
    }
}

/// `serde_json`'s recursion limit bounds the depth.
fn vertices(path: &Value) -> u64 {
    match path {
        Value::Array(items) => items.iter().map(vertices).max().unwrap_or(0),
        Value::Object(fields) => {
            let own = fields
                .get("v")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let keyed = ["k", "s", "e"]
                .iter()
                .filter_map(|key| fields.get(*key))
                .map(vertices)
                .max()
                .unwrap_or(0);
            keyed.max(own as u64)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 0,
    }
}

fn repeats(shape: &Map<String, Value>, property: &str) -> Result<u64, Malformed> {
    let repeats = shape.get(property).map_or(0.0, peak);
    if repeats > MAX_REPEATS {
        return Err(Malformed::TooComplex);
    }
    Ok(repeats as u64)
}

#[derive(Clone, Copy, Default)]
struct Cost {
    work: u64,
    levels_below: usize,
}

enum Visit {
    NotYet,
    Open,
    Done(Cost),
}

struct Asset<'a> {
    layers: &'a [Value],
    visit: Visit,
}

struct Walk<'a> {
    assets: Vec<Asset<'a>>,
    /// Assets by ThorVG's key for their id, first one wins, as in its
    /// lookup, so this walks the graph ThorVG will build.
    by_key: HashMap<c_ulong, usize>,
    layers_seen: usize,
    glyph_work: u64,
}

/// ThorVG's key for an id (`djb2Encode`): it stops at a NUL, and its width
/// and the sign of its bytes follow the platform's C types.
fn asset_key(id: &str) -> c_ulong {
    id.bytes()
        .take_while(|byte| *byte != 0)
        .fold(5381, |hash: c_ulong, byte| {
            hash.wrapping_mul(33)
                .wrapping_add(c_int::from(byte as c_char) as c_ulong)
        })
}

impl<'a> Walk<'a> {
    fn new(doc: &'a Value) -> Result<Self, Malformed> {
        let mut walk = Self {
            assets: Vec::new(),
            by_key: HashMap::new(),
            layers_seen: 0,
            glyph_work: glyph_work(doc)?,
        };
        let Some(assets) = doc.get("assets") else {
            return Ok(walk);
        };
        for asset in assets.as_array().ok_or(Malformed::Asset)? {
            let asset = asset.as_object().ok_or(Malformed::Asset)?;
            let Some(layers) = asset.get("layers") else {
                continue;
            };
            let layers = layers.as_array().ok_or(Malformed::Asset)?;
            let index = walk.assets.len();
            walk.assets.push(Asset {
                layers,
                visit: Visit::NotYet,
            });
            match asset.get("id") {
                None => {}
                Some(Value::String(id)) => {
                    walk.by_key.entry(asset_key(id)).or_insert(index);
                }
                Some(Value::Number(id)) => {
                    let id = id.as_i64().ok_or(Malformed::Asset)?;
                    walk.by_key
                        .entry(asset_key(&id.to_string()))
                        .or_insert(index);
                }
                Some(_) => return Err(Malformed::Asset),
            }
        }
        Ok(walk)
    }

    fn layers(&mut self, layers: &'a [Value], depth: usize) -> Result<Cost, Malformed> {
        self.layers_seen = self.layers_seen.saturating_add(layers.len());
        if self.layers_seen > MAX_LAYERS {
            return Err(Malformed::TooComplex);
        }
        let mut total = Cost::default();
        for layer in layers {
            let layer = layer.as_object().ok_or(Malformed::Layer)?;
            let cost = self.layer(layer, depth)?;
            total.work = total
                .work
                .saturating_add(SHAPE_WORK)
                .saturating_add(cost.work);
            total.levels_below = total.levels_below.max(cost.levels_below);
        }
        Ok(total)
    }

    fn layer(&mut self, layer: &'a Map<String, Value>, depth: usize) -> Result<Cost, Malformed> {
        let kind = match layer.get("ty") {
            Some(kind) => Some(
                kind.as_u64()
                    .filter(|kind| *kind <= MAX_LAYER_KIND)
                    .ok_or(Malformed::Layer)?,
            ),
            None => None,
        };
        let precomp = kind == Some(PRECOMP);
        if precomp && (layer.contains_key("shapes") || layer.contains_key("t")) {
            return Err(Malformed::Layer);
        }
        let mut cost = Cost::default();
        if let Some(masks) = layer.get("masksProperties") {
            let masks = masks.as_array().ok_or(Malformed::Layer)?;
            if masks.len() > MAX_MASKS {
                return Err(Malformed::TooComplex);
            }
            cost.work = masks.iter().fold(0, |work: u64, mask| {
                let path = mask.get("pt").map_or(0, vertices);
                work.saturating_add(path.saturating_mul(VERTEX_WORK))
                    .saturating_add(OFFSCREEN_WORK)
            });
        }
        let effects = layer.get("ef").and_then(Value::as_array);
        let drawn = effects.into_iter().flatten();
        let drawn =
            drawn.filter(|effect| effect.get("ty").and_then(Value::as_u64) != Some(CUSTOM_EFFECT));
        let matte = ["tt", "td"].iter().any(|key| layer.contains_key(*key));
        let offscreen = drawn.count() as u64 + u64::from(matte);
        cost.work = cost
            .work
            .saturating_add(offscreen.saturating_mul(OFFSCREEN_WORK));
        if kind == Some(TEXT) {
            cost.work = cost
                .work
                .saturating_add(text_length(layer).saturating_mul(self.glyph_work));
        }
        if let Some(shapes) = layer.get("shapes") {
            let shapes = shape_list(shapes.as_array().ok_or(Malformed::Layer)?, 0)?;
            cost.work = cost.work.saturating_add(shapes);
        }
        if let Some(reference) = layer.get("refId") {
            let reference = reference.as_str().ok_or(Malformed::Layer)?;
            if precomp && let Some(index) = self.by_key.get(&asset_key(reference)).copied() {
                let shown = self.asset(index, depth + 1)?;
                cost.work = cost.work.saturating_add(shown.work);
                cost.levels_below = shown.levels_below + 1;
            }
        }
        Ok(cost)
    }

    fn asset(&mut self, index: usize, depth: usize) -> Result<Cost, Malformed> {
        let Some(asset) = self.assets.get_mut(index) else {
            return Ok(Cost::default());
        };
        let cost = match asset.visit {
            Visit::Done(cost) => cost,
            Visit::Open => return Err(Malformed::Asset),
            Visit::NotYet if depth > MAX_PRECOMP_DEPTH => return Err(Malformed::TooComplex),
            Visit::NotYet => {
                asset.visit = Visit::Open;
                let layers = asset.layers;
                let cost = self.layers(layers, depth)?;
                if let Some(asset) = self.assets.get_mut(index) {
                    asset.visit = Visit::Done(cost);
                }
                cost
            }
        };
        if lands_too_deep(depth, cost) {
            return Err(Malformed::TooComplex);
        }
        Ok(cost)
    }
}

fn lands_too_deep(depth: usize, asset: Cost) -> bool {
    depth.saturating_add(asset.levels_below) > MAX_PRECOMP_DEPTH
}

fn shape_list(shapes: &[Value], depth: usize) -> Result<u64, Malformed> {
    if depth > MAX_GROUP_DEPTH {
        return Err(Malformed::TooComplex);
    }
    let (mut cost, mut copies, mut paints) = (0u64, 1u64, 0u64);
    // Repeaters and path modifiers act on what comes before them in the list.
    for shape in shapes.iter().rev() {
        let shape = shape.as_object().ok_or(Malformed::Shape)?;
        let kind = match shape.get("ty") {
            Some(kind) => Some(kind.as_str().ok_or(Malformed::Shape)?),
            None => None,
        };
        let own = match kind {
            Some("rp") => {
                copies = copies.saturating_mul(repeats(shape, "c")?.max(1));
                continue;
            }
            Some("zz") => {
                copies = copies.saturating_mul(repeats(shape, "r")? * 2 + 2);
                continue;
            }
            Some("op") => {
                copies = copies.saturating_mul(OFFSET_COPIES);
                continue;
            }
            Some("rd" | "pb") => {
                copies = copies.saturating_mul(2);
                continue;
            }
            Some("gr") => match shape.get("it") {
                Some(items) => shape_list(items.as_array().ok_or(Malformed::Shape)?, depth + 1)?,
                None => 0,
            },
            Some("sr") => path_work(repeats(shape, "pt")? * 2),
            Some("sh") => path_work(shape.get("ks").map_or(0, vertices)),
            Some("rc" | "el") => path_work(4),
            Some("fl") => {
                paints += 1;
                0
            }
            Some("gf") => {
                check_gradient(shape)?;
                paints += 1;
                0
            }
            Some("st") => {
                paints += stroke_paints(shape);
                0
            }
            Some("gs") => {
                check_gradient(shape)?;
                paints += stroke_paints(shape);
                0
            }
            _ => 0,
        };
        cost = cost.saturating_add(copies.saturating_mul(own));
    }
    Ok(cost.saturating_mul(paints.max(1)))
}

fn stroke_paints(stroke: &Map<String, Value>) -> u64 {
    if stroke.contains_key("d") {
        DASHED_PAINTS
    } else {
        1
    }
}

fn path_work(vertices: u64) -> u64 {
    vertices
        .saturating_mul(VERTEX_WORK)
        .saturating_add(SHAPE_WORK)
}

fn glyph_work(doc: &Value) -> Result<u64, Malformed> {
    let Some(glyphs) = doc.get("chars").and_then(Value::as_array) else {
        return Ok(0);
    };
    let mut work = 0;
    for glyph in glyphs {
        let shapes = glyph.get("data").and_then(|data| data.get("shapes"));
        if let Some(shapes) = shapes.and_then(Value::as_array) {
            work = shape_list(shapes, 0)?.max(work);
        }
    }
    Ok(work)
}

fn text_length(layer: &Map<String, Value>) -> u64 {
    let keyframes = layer
        .get("t")
        .and_then(|text| text.get("d"))
        .and_then(|document| document.get("k"))
        .and_then(Value::as_array);
    keyframes
        .into_iter()
        .flatten()
        .filter_map(|keyframe| keyframe.get("s")?.get("t")?.as_str())
        .map(|text| text.len() as u64)
        .max()
        .unwrap_or(0)
}

fn check_gradient(shape: &Map<String, Value>) -> Result<(), Malformed> {
    let stops = shape.get("g").and_then(|gradient| gradient.get("p"));
    match stops {
        Some(stops) if !stops.as_u64().is_some_and(|n| n <= MAX_GRADIENT_STOPS) => {
            Err(Malformed::Shape)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "lottie_check_tests.rs"]
mod tests;

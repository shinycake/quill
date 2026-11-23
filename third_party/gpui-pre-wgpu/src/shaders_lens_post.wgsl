// Added by the Quill project (2026). See third_party/gpui-pre-wgpu/QUILL-CHANGES.md.
//
// Backdrop glass's own passes: the dual-Kawase downsamples that fill the
// lens atlas's mip chain (`lens_downsample_fragment` in gpui-pre-apple's
// shaders.metal), and the copy of a frame drawn offscreen onto a surface
// that can't be read back (`lens.rs`, `FrameCopy`).

// One slot of one downsample pass: the texels it writes in the level being
// filled, and the texels of the level above it may read
// (`LensDownsampleSlot` in lens.rs).
struct LensDownsampleSlot {
    dst_origin: vec2<f32>,
    dst_size: vec2<f32>,
    src_min: vec2<f32>,
    src_max: vec2<f32>,
}

@group(0) @binding(0) var<storage, read> b_down_slots: array<LensDownsampleSlot>;
@group(0) @binding(1) var t_down_source: texture_2d<f32>;
@group(0) @binding(2) var s_down_source: sampler;

struct DownsampleVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) slot_id: u32,
}

@vertex
fn vs_lens_downsample(@builtin(vertex_index) vertex_id: u32, @builtin(instance_index) instance_id: u32) -> DownsampleVarying {
    let unit_vertex = vec2<f32>(f32(vertex_id & 1u), 0.5 * f32(vertex_id & 2u));
    let slot = b_down_slots[instance_id];
    // The level written is half the level read (the atlas's sides are
    // multiples of 256, so every level halves exactly).
    let target_size = vec2<f32>(textureDimensions(t_down_source, 0)) * 0.5;
    let position = slot.dst_origin + unit_vertex * slot.dst_size;
    var out = DownsampleVarying();
    out.position = vec4<f32>(position / target_size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.slot_id = instance_id;
    return out;
}

// Dual-Kawase downsample: the centre and four diagonal taps one source
// texel away, each a bilinear average of four source texels, kept inside
// the slot's part of the source level.
@fragment
fn fs_lens_downsample(input: DownsampleVarying) -> @location(0) vec4<f32> {
    let slot = b_down_slots[input.slot_id];
    let source_size = vec2<f32>(textureDimensions(t_down_source, 0));
    let lo = slot.src_min + 0.5;
    let hi = max(slot.src_max - 0.5, lo);
    let c = input.position.xy * 2.0;
    var sum = textureSampleLevel(t_down_source, s_down_source, clamp(c, lo, hi) / source_size, 0.0) * 4.0;
    sum += textureSampleLevel(t_down_source, s_down_source, clamp(c + vec2<f32>(1.0, 1.0), lo, hi) / source_size, 0.0);
    sum += textureSampleLevel(t_down_source, s_down_source, clamp(c + vec2<f32>(-1.0, 1.0), lo, hi) / source_size, 0.0);
    sum += textureSampleLevel(t_down_source, s_down_source, clamp(c + vec2<f32>(1.0, -1.0), lo, hi) / source_size, 0.0);
    sum += textureSampleLevel(t_down_source, s_down_source, clamp(c + vec2<f32>(-1.0, -1.0), lo, hi) / source_size, 0.0);
    return sum / 8.0;
}

@group(0) @binding(3) var t_frame: texture_2d<f32>;

@vertex
fn vs_lens_frame_copy(@builtin(vertex_index) vertex_id: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let p = vec2<f32>(f32((vertex_id << 1u) & 2u), f32(vertex_id & 2u));
    return vec4<f32>(p * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

@fragment
fn fs_lens_frame_copy(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(t_frame, vec2<i32>(position.xy), 0);
}

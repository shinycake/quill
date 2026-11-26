// Added by the Quill project (2026). See third_party/gpui-pre-wgpu/QUILL-CHANGES.md.
//
// Backdrop glass (`BackdropLens`), concatenated after `shaders.wgsl` (for
// `globals`, `Bounds`, `Hsla`, `hsla_to_rgba`, `to_device_position`,
// `distance_from_clip_rect` and `blend_color`) and `shaders_storage.wgsl`
// (which `shaders.wgsl` calls). The renderer copies each
// lens's footprint out of the frame into a slot of a mip-chained atlas and
// blurs the chain with dual-Kawase downsamples (`shaders_lens_post.wgsl`);
// `fs_lens` draws the lens from it. A port of `lens_fragment` in
// gpui-pre-apple's shaders.metal: the optics are gpui-liquid-glass's CPU
// reference (`optics.rs`, after bilalgursen/liquid-glass, MIT), in design
// units of a lens 120 units tall.

const LENS_LEVELS: i32 = 5;

// QUILL: instance layouts, in bytes: each struct's size and the offset of
// its `content_mask`. src/ui/rounded_mask_tests.rs checks these lines
// against the Rust structs, so a struct changed on one side fails a test.
// QUILL-LAYOUT BackdropLens size=328 mask=24

// `gpui::BackdropLens`, field for field (`lens.rs` asserts the size and
// the offsets the vec2 alignment of `Bounds` could move).
struct BackdropLens {
    order: u32,
    material: u32,
    bounds: Bounds,
    content_mask: ContentMask,
    body: Bounds,
    corner_radius: f32,
    scale_factor: f32,
    unit: f32,
    bevel: f32,
    band: f32,
    shadow: f32,
    shadow_softness: f32,
    shadow_offset: f32,
    strength: f32,
    opacity: f32,
    color: Hsla,
    surface: Hsla,
    bend: u32,
    refract: f32,
    magnify: f32,
    dispersion: f32,
    blur_center: f32,
    blur_edge: f32,
    frost: f32,
    blur: f32,
    blur_mix: f32,
    radial_amount: f32,
    edge_clear: f32,
    border_color: Hsla,
    border_width: f32,
    tune_rim: f32,
    tune_rim_width: f32,
    tune_fresnel: f32,
    tune_inner: f32,
    tune_reflection: f32,
    tune_caustic: f32,
    tune_bend_curve: f32,
    tune_bend_depth: f32,
    tune_saturation: f32,
    light_x: f32,
    light_y: f32,
    light_mix: f32,
    // QUILL (glass stage 5).
    id: u32,
    density: u32,
    drop_x: f32,
    drop_r: f32,
    drop_k: f32,
    blob_x: f32,
    blob_y: f32,
    blob_hx: f32,
    blob_hy: f32,
    blob_r: f32,
    blob_k: f32,
    neck_x: f32,
    neck_w: f32,
    tune_frost_edge: f32,
}

// Where one lens's backdrop lies in the frame and in the atlas, device
// pixels (`LensSlot` in lens.rs).
struct LensSlot {
    copy_origin: vec2<f32>,
    copy_size: vec2<f32>,
    slot_origin: vec2<f32>,
    // Atlas texels per device pixel (1: copied from the frame).
    density: f32,
    pad: f32,
}

// Group 1's bindings 0 to 3 are the surfaces' in shaders.wgsl.
@group(1) @binding(4) var<storage, read> b_lenses: array<BackdropLens>;
@group(1) @binding(5) var<storage, read> b_lens_slots: array<LensSlot>;
@group(1) @binding(6) var t_lens_atlas: texture_2d<f32>;
@group(1) @binding(7) var s_lens_atlas: sampler;

struct LensVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) lens_id: u32,
    @location(1) clip_distances: vec4<f32>,
}

@vertex
fn vs_lens(@builtin(vertex_index) vertex_id: u32, @builtin(instance_index) instance_id: u32) -> LensVarying {
    let unit_vertex = vec2<f32>(f32(vertex_id & 1u), 0.5 * f32(vertex_id & 2u));
    let lens = b_lenses[instance_id];
    var out = LensVarying();
    out.position = to_device_position(unit_vertex, lens.bounds);
    out.lens_id = instance_id;
    let position = unit_vertex * vec2<f32>(lens.bounds.size) + lens.bounds.origin;
    out.clip_distances = distance_from_clip_rect_impl(position, lens.content_mask.bounds);
    return out;
}

// `smoothstep` as optics.rs has it: either edge may be the larger one.
fn lens_step(a: f32, b: f32, v: f32) -> f32 {
    let t = clamp((v - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn lens_dim(k: f32) -> f32 {
    return pow(clamp(k, 0.0, 1.0), 1.0 / 2.2);
}

fn lens_round_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn lens_saturate(c: vec3<f32>, s: f32) -> vec3<f32> {
    if (abs(s - 1.0) < 1e-4) {
        return c;
    }
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return max(mix(vec3<f32>(l), c, s), vec3<f32>(0.0));
}

// Everything per lens the pixels share (optics.rs `Prepared`). Lengths are
// units.
struct LensShape {
    center: vec2<f32>,  // device pixels
    unit_dev: f32,      // device pixels per unit
    device_px: f32,     // a device pixel, in units
    css_px: f32,        // a logical pixel, in units
    half_size: vec2<f32>,
    radius: f32,
    field_radius: f32,
    bevel: f32,
    band: f32,
    edge_band: f32,
    blur_shift: f32,
    // QUILL (glass stage 5): gooey shapes, units from the body's centre (see
    // `lens_sdf`), and the frost's multiplier at the rim.
    drop: vec3<f32>,       // x, radius, bridge softness; radius 0 = none
    blob: vec4<f32>,       // centre x, y, half width, half height; 0 = none
    blob_shape: vec2<f32>, // corner radius, bridge softness
    blob_neck: vec2<f32>,  // x, falloff width; 0 = no neck
    frost_edge: f32,
}

fn lens_shape(lens: BackdropLens) -> LensShape {
    var s: LensShape;
    let scale = max(lens.scale_factor, 1e-3);
    let unit = max(lens.unit, 1e-3);
    s.unit_dev = unit * scale;
    s.device_px = 1.0 / s.unit_dev;
    s.css_px = 1.0 / unit;
    s.center = lens.body.origin + lens.body.size * 0.5;
    let half_logical = lens.body.size * 0.5 / scale;
    s.half_size = max(half_logical / unit, vec2<f32>(0.5));
    let radius = min(lens.corner_radius / scale, min(half_logical.x - 0.5, half_logical.y - 0.5));
    s.radius = max(radius, 0.0) / unit;
    s.band = select(0.0, lens.band, lens.material == 0u);
    let min_half = max(min(half_logical.x, half_logical.y) - 0.5, 0.0) / unit;
    s.field_radius = min(max(s.radius, s.band), min_half);
    s.bevel = max(lens.bevel, 2.0);
    s.edge_band = 0.35 * max(lens.tune_bend_depth, 0.05);
    s.blur_shift = log2(max(s.unit_dev / 2.27, 1e-3));
    s.drop = vec3<f32>(lens.drop_x, lens.drop_r, lens.drop_k) / unit;
    s.blob = vec4<f32>(lens.blob_x, lens.blob_y, lens.blob_hx, lens.blob_hy) / unit;
    s.blob_shape = vec2<f32>(lens.blob_r, lens.blob_k) / unit;
    s.blob_neck = vec2<f32>(lens.neck_x, lens.neck_w) / unit;
    s.frost_edge = max(lens.tune_frost_edge, 0.05);
    return s;
}

// QUILL (glass stage 5): the source kit's smooth union (`melt`).
fn lens_melt(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (a - b) / k, 0.0, 1.0);
    return mix(a, b, h) - k * h * (1.0 - h);
}

// The blob's bridge softness at `p`: with a neck it peaks there and falls
// off, so a thinning bridge narrows into a thread before it breaks.
fn lens_blob_soft(s: LensShape, p: vec2<f32>) -> f32 {
    var k = s.blob_shape.y;
    if (s.blob_neck.y > 0.0) {
        let n = (p.x - s.blob_neck.x) / s.blob_neck.y;
        k *= exp(0.2 - sqrt(n * n + 0.04));
    }
    return max(k, 1e-3);
}

fn lens_with_blob(s: LensShape, p: vec2<f32>, body: f32) -> f32 {
    if (s.blob.z <= 0.0 || s.blob.w <= 0.0) {
        return body;
    }
    let blob = lens_round_rect(p - s.blob.xy, s.blob.zw, s.blob_shape.x);
    return lens_melt(blob, body, lens_blob_soft(s, p));
}

// The glass: the body, melted with its droplet and its blob.
fn lens_sdf(s: LensShape, p: vec2<f32>) -> f32 {
    var body = lens_round_rect(p, s.half_size, s.radius);
    if (s.drop.y > 0.0) {
        let d = length(p - vec2<f32>(s.drop.x, 0.0)) - s.drop.y;
        body = lens_melt(d, body, max(s.drop.z, 1e-3));
    }
    return lens_with_blob(s, p, body);
}

// The body with corners as round as the bend is deep, and its blob (the
// droplet bends on its own rim only, as in the source kit).
fn lens_field(s: LensShape, p: vec2<f32>) -> f32 {
    return lens_with_blob(s, p, lens_round_rect(p, s.half_size, s.field_radius));
}

fn lens_height(s: LensShape, p: vec2<f32>) -> f32 {
    let t = pow(clamp(-lens_sdf(s, p) / s.bevel, 0.0, 1.0), 1.0 / 1.4);
    return sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
}

// What is behind the glass at `p` (units from the lens centre), blurred to
// mip level `lod`. Taps stay half a texel of that level inside the copied
// footprint, so the gutter between slots never shows.
fn lens_back(s: LensShape, slot: LensSlot, atlas_size: vec2<f32>, p: vec2<f32>, lod: f32) -> vec3<f32> {
    // A re-rendered slot holds `density` texels per device pixel: the same
    // blur is that many levels further down, on whole levels (its levels
    // below the sharpest are blurred, not just smaller).
    let density = max(slot.density, 1.0);
    var mip = clamp(lod + log2(density), 0.0, f32(LENS_LEVELS - 1));
    if (density > 1.0) {
        mip = floor(mip);
    }
    let window = s.center + p * s.unit_dev;
    let inset = 0.5 * exp2(ceil(mip)) / density;
    let lo = vec2<f32>(inset);
    let hi = max(slot.copy_size - inset, lo);
    let local = clamp(window - slot.copy_origin, lo, hi);
    let uv = (slot.slot_origin + local * density) / atlas_size;
    return textureSampleLevel(t_lens_atlas, s_lens_atlas, uv, mip).rgb;
}

// A gaussian of `sigma` device pixels, from the mip chain: level k holds
// about sigma 0.87 * 2^k, eight taps around the point widen it.
fn lens_frosted(s: LensShape, slot: LensSlot, atlas_size: vec2<f32>, p: vec2<f32>, sigma: f32) -> vec3<f32> {
    let lod = log2(max(sigma, 0.5) / 0.87);
    let mip = clamp(lod, 0.0, f32(LENS_LEVELS - 1));
    let reach = 0.75 * exp2(mip) * max(exp2(lod - mip), 1.0) / s.unit_dev;
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let angle = f32(i) * (M_PI_F / 4.0) + 0.3;
        let r = select(reach * 0.55, reach, i % 2 == 0);
        sum += lens_back(s, slot, atlas_size, p + vec2<f32>(cos(angle), sin(angle)) * r, mip);
    }
    return sum / 8.0;
}

fn lens_bent(s: LensShape, slot: LensSlot, atlas_size: vec2<f32>, p: vec2<f32>, bend: vec2<f32>, split: f32, lod: f32) -> vec3<f32> {
    let r = lens_back(s, slot, atlas_size, p + bend * (1.0 + split), lod).r;
    let g = lens_back(s, slot, atlas_size, p + bend, lod).g;
    let b = lens_back(s, slot, atlas_size, p + bend * (1.0 - split), lod).b;
    return vec3<f32>(r, g, b);
}

fn lens_radial_bend(s: LensShape, lens: BackdropLens, q: vec2<f32>, amount: f32) -> vec2<f32> {
    let d = lens_sdf(s, q);
    let len = max(length(q), 1e-4);
    let ratio = clamp(1.0 - abs(d) / (min(s.half_size.x, s.half_size.y) * s.edge_band / 0.35), 0.0, 1.0);
    let k = pow(ratio, lens.tune_bend_curve) * lens_step(1.5 * s.css_px, -0.5 * s.css_px, d);
    return -q / len * k * amount;
}

@fragment
fn fs_lens(input: LensVarying) -> @location(0) vec4<f32> {
    // Alpha clip first, since there is no `clip_distance`.
    if (any(input.clip_distances < vec4<f32>(0.0))) {
        discard;
    }
    let lens = b_lenses[input.lens_id];
    let slot = b_lens_slots[input.lens_id];
    let mask_coverage = rounded_mask_coverage(input.position.xy, lens.content_mask.rounded);
    let atlas_size = vec2<f32>(textureDimensions(t_lens_atlas, 0));
    let s = lens_shape(lens);
    let strength = lens.strength;
    let clear = lens.material == 0u;

    let local = (input.position.xy - s.center) / s.unit_dev;
    let dist = lens_sdf(s, local);

    // Drop shadow: tight + ambient, faded before the footprint's edge.
    let v_fade = lens_step(-0.1, 0.9, local.y / s.half_size.y) * 0.92 + 0.08;
    let outside = lens_step(-4.0, 14.0, dist);
    let soft = lens.shadow_softness;
    let off = lens.shadow_offset;
    let near = 1.0 - lens_step(-6.0, soft, lens_sdf(s, local - vec2<f32>(0.0, off)));
    let far = 1.0 - lens_step(-10.0, soft * 1.5, lens_sdf(s, local - vec2<f32>(0.0, off * 1.6)));
    let aa = max(s.device_px * 1.2, 1e-4);
    let mask = 1.0 - lens_step(-aa, aa, dist);
    let in_footprint = input.position.xy - lens.bounds.origin;
    let border = min(min(in_footprint.x, in_footprint.y),
                     min(lens.bounds.size.x - in_footprint.x, lens.bounds.size.y - in_footprint.y));
    let sh = clamp((near + far) * lens.shadow * strength * v_fade * outside, 0.0, 0.85)
        * lens_step(0.0, 14.0, border);
    let shadow_alpha = 1.0 - lens_dim(1.0 - sh);
    if (mask <= 0.0) {
        if (shadow_alpha <= 0.0) {
            discard;
        }
        return blend_color(vec4<f32>(0.0, 0.0, 0.0, shadow_alpha), lens.opacity * mask_coverage);
    }

    // Surface normal from the bevel height field.
    let e = 1.25;
    let h = lens_height(s, local);
    let hx = lens_height(s, local + vec2<f32>(e, 0.0));
    let hy = lens_height(s, local + vec2<f32>(0.0, e));
    let n = vec3<f32>(-(hx - h) / e * 42.0, -(hy - h) / e * 42.0, 1.0);
    let normal_z = n.z / length(n);
    let depth = clamp(1.0 - h, 0.0, 1.0);
    let edge = 1.0 - lens_step(0.0, s.bevel * s.edge_band, -dist);
    let dxy = vec2<f32>(lens_sdf(s, local + vec2<f32>(e, 0.0)) - dist,
                        lens_sdf(s, local + vec2<f32>(0.0, e)) - dist);
    let dir = dxy / max(length(dxy), 1e-6);

    let color = hsla_to_rgba(lens.color);
    let surface = hsla_to_rgba(lens.surface);
    let frost_sigma = select(0.0, max(lens.blur, 0.0) * lens.scale_factor, clear);
    var glass: vec3<f32>;
    if (!clear) {
        glass = mix(surface.rgb, color.rgb, 1.0 - edge * lens.edge_clear);
    } else if (lens.bend == 1u) {
        let amount = lens.radial_amount / max(lens.unit, 1e-3);
        let bend = lens_radial_bend(s, lens, local, amount);
        glass = lens_bent(s, slot, atlas_size, local, bend, 0.0, max(s.blur_shift, 0.0));
        if (frost_sigma > 0.0) {
            let ratio = clamp(1.0 - abs(dist) / (min(s.half_size.x, s.half_size.y) * s.edge_band / 0.35), 0.0, 1.0);
            let sigma = frost_sigma * mix(1.0, s.frost_edge, pow(ratio, 1.3));
            let frost = lens_frosted(s, slot, atlas_size, local + bend, max(sigma, 0.5));
            glass = mix(glass, frost, lens.blur_mix);
        }
        glass = lens_saturate(glass, lens.tune_saturation);
    } else {
        var bend_edge = edge;
        var bend_dir = dir;
        if (s.band > 0.0) {
            let fd = lens_field(s, local);
            let fxy = vec2<f32>(lens_field(s, local + vec2<f32>(e, 0.0)) - fd,
                                lens_field(s, local + vec2<f32>(0.0, e)) - fd);
            bend_edge = 1.0 - lens_step(0.0, s.band, -fd);
            bend_dir = fxy / max(length(fxy), 1e-6);
        }
        let mag = lens.magnify * h + 1.0;
        let at = local / mag;
        let k = lens.refract * pow(bend_edge, lens.tune_bend_curve);
        let bend = -bend_dir * k;
        let split = lens.dispersion * 0.35;
        // QUILL (glass stage 5): the blur is measured where it shows, so a
        // magnified middle reads its backdrop that much finer.
        let lod = max(mix(lens.blur_center, lens.blur_edge, pow(bend_edge, 1.3)) + s.blur_shift, 0.0)
            - log2(mag);
        glass = lens_bent(s, slot, atlas_size, at, bend, split, lod);
        if (frost_sigma > 0.0) {
            let sigma = frost_sigma * mix(1.0, s.frost_edge, pow(bend_edge, 1.3));
            let frost = lens_frosted(s, slot, atlas_size, at + bend, max(sigma, 0.5));
            glass = mix(glass, frost, lens.blur_mix);
        }
        glass = lens_saturate(glass, lens.tune_saturation);
    }

    // Top reflection.
    let ny = local.y / s.half_size.y;
    let nx = local.x / s.half_size.x;
    let top_refl = 0.3 * strength * lens.tune_reflection;
    let top_base = 0.02 * strength * lens.tune_reflection;
    let top = (1.0 - lens_step(-0.05, 0.75, ny)) * (1.0 - nx * 0.12) * top_refl * 0.6 + top_base;
    glass *= vec3<f32>(lens_dim(1.0 - top), lens_dim(1.0 - top), lens_dim(1.0 - 0.72 * top));

    // Frosted resting state. QUILL (glass effects): `edge_clear` thins the
    // frost across the rim band (`LensTune::rim_frost`).
    if (clear) {
        glass = mix(glass, color.rgb, lens.frost * (1.0 - edge * lens.edge_clear));
    }

    // Inner shading: just inside the rim, a wide band on the left, a deep ring.
    let inner = 0.1 * strength * lens.tune_inner;
    let up = clamp(-dir.y, 0.0, 1.0);
    let right = clamp(dir.x, 0.0, 1.0);
    let left = clamp(-dir.x, 0.0, 1.0);
    let down = clamp(dir.y, 0.0, 1.0);
    let inner_band = lens_step(s.bevel * -0.19, s.bevel * -0.024, dist);
    let k1 = lens_dim(1.0 - inner_band * (up + left * 0.6 + right * 0.3 + down * 0.15) * inner * 2.2);
    let deep_band = lens_step(s.bevel * -0.73, s.bevel * -0.13, dist);
    let k2 = lens_dim(1.0 - deep_band * left * inner * 2.0);
    let hxr = (dist + s.bevel * 0.45) / (s.bevel * 0.26);
    let ring = exp(-hxr * hxr);
    let k3 = lens_dim(1.0 - ring * (right * 0.9 + up * 0.5 + left * 0.4 + down * 0.3) * inner * 1.5);
    glass *= k1 * k2 * k3;

    // Rim light + fresnel, leaning towards the pointer.
    let rim = 0.55 * strength * lens.tune_rim;
    let fresnel = pow(clamp(1.0 - normal_z, 0.0, 1.0), 6.0);
    let rim_w = max(s.device_px * 1.1, 1.0) * max(lens.tune_rim_width, 0.05);
    let rim_line = 1.0 - lens_step(0.0, rim_w * 1.6, abs(dist + rim_w * 1.8));
    let fall = exp(-(local.y / 30.0) * (local.y / 30.0));
    let studio = right * right * fall * 0.9 + down * down * 0.2 + left * left * 0.55;
    let light = vec2<f32>(lens.light_x, lens.light_y);
    let light_dir = select(vec2<f32>(0.0, -1.0), normalize(light), length(light) > 1e-3);
    let light_cos = dot(dir, light_dir);
    let facing = clamp(light_cos, 0.0, 1.0);
    let away = clamp(-light_cos, 0.0, 1.0);
    let lit = facing * facing * 0.9 + away * away * 0.55;
    let lean = lens.light_mix * 0.4;
    let rim_dir = mix(studio, lit, lean);
    let front = mix(right, facing, lean);
    let back_side = mix(left, away, lean);
    let warm = 1.0 - lens_step(-0.5, 0.1, nx);
    let warm_cool = mix(vec3<f32>(0.82, 0.9, 1.08), vec3<f32>(1.2, 0.95, 0.78), warm);
    var rim_col = mix(vec3<f32>(1.0), warm_cool, down * 0.6);
    rim_col = mix(rim_col, vec3<f32>(0.12, 0.6, 1.2), back_side * 0.85);
    let lit_on = select(0.0, 1.0, rim > 0.001);
    let add = rim_line * rim_dir * rim + fresnel * front * 0.02 * lens.tune_fresnel * lit_on;
    // Caustic: warm light gathering inside the bottom edge.
    let caustic = lens_step(0.72, 0.98, depth) * down * 0.08 * strength * lens.tune_caustic;
    glass += rim_col * add + vec3<f32>(1.0, 0.92, 0.86) * caustic;

    let border_color = hsla_to_rgba(lens.border_color);
    let border_w = lens.border_width / max(lens.unit, 1e-3);
    if (border_w > 1e-4 && border_color.a > 0.0) {
        let t = lens_step(-border_w - aa, -border_w + aa, dist) * border_color.a;
        glass = mix(glass, border_color.rgb, t);
    }

    // The glass over its shadow, straight alpha.
    let g = clamp(glass, vec3<f32>(0.0), vec3<f32>(1.0));
    let a = mask + shadow_alpha * (1.0 - mask);
    if (a <= 1e-5) {
        discard;
    }
    return blend_color(vec4<f32>(g * mask / a, a), lens.opacity * mask_coverage);
}

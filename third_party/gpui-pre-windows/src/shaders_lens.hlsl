// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//
// Backdrop glass (`BackdropLens`), compiled from this file on its own so a
// shader error here can only make the glass draw flat, never the window's
// other shaders fail. It includes `shaders.hlsl` for `GlobalParams`,
// `BatchParams`, `Bounds`, `Hsla`, `hsla_to_rgba`, `to_device_position`,
// `distance_from_clip_rect`, `to_output` and the exact sRGB curves.
//
// The renderer copies each lens's footprint out of the frame into a slot of
// a mip-chained atlas, fills the levels with dual-Kawase downsamples
// (`lens_downsample_*`) and draws the lens from it (`lens_*`). A port of
// `fs_lens` in gpui-pre-wgpu's shaders_lens.wgsl, itself a port of
// `lens_fragment` in gpui-pre-apple's shaders.metal: the optics are
// gpui-liquid-glass's CPU reference (`optics.rs`, after
// bilalgursen/liquid-glass, MIT), in design units of a lens 120 units tall.
//
// Bindings: t1 the instances (lenses, or the downsample slots), t2 the
// lenses' atlas slots, t3 the atlas (the whole chain for the lens, the level
// read for a downsample), s1 a clamped trilinear sampler.
#include "shaders.hlsl"

static const int LENS_LEVELS = 5;

// `gpui::BackdropLens`, field for field (296 bytes, no padding: structured
// buffers pack at 4 bytes).
struct BackdropLens {
    uint order;
    uint material;
    Bounds bounds;
    Bounds content_mask;
    Bounds body;
    float corner_radius;
    float scale_factor;
    float unit;
    float bevel;
    float band;
    float shadow;
    float shadow_softness;
    float shadow_offset;
    float strength;
    float opacity;
    Hsla color;
    Hsla surface;
    uint bend;
    float refract;
    float magnify;
    float dispersion;
    float blur_center;
    float blur_edge;
    float frost;
    float blur;
    float blur_mix;
    float radial_amount;
    float edge_clear;
    Hsla border_color;
    float border_width;
    float tune_rim;
    float tune_rim_width;
    float tune_fresnel;
    float tune_inner;
    float tune_reflection;
    float tune_caustic;
    float tune_bend_curve;
    float tune_bend_depth;
    float tune_saturation;
    float light_x;
    float light_y;
    float light_mix;
    // QUILL (glass stage 5).
    uint id;
    uint density;
    float drop_x;
    float drop_r;
    float drop_k;
    float blob_x;
    float blob_y;
    float blob_hx;
    float blob_hy;
    float blob_r;
    float blob_k;
    float neck_x;
    float neck_w;
    float tune_frost_edge;
};

// Where one lens's backdrop lies in the frame and in the atlas, device
// pixels (`LensSlot` in backdrop_glass.rs).
struct LensSlot {
    float2 copy_origin;
    float2 copy_size;
    float2 slot_origin;
    // Atlas texels per device pixel (1: copied from the frame).
    float density;
    float pad;
};

// One slot of one downsample pass: the texels it writes in the level being
// filled, and the texels of the level above it may read
// (`LensDownsampleSlot` in backdrop_glass.rs).
struct LensDownsampleSlot {
    float2 dst_origin;
    float2 dst_size;
    float2 src_min;
    float2 src_max;
};

StructuredBuffer<BackdropLens> b_lenses: register(t1);
StructuredBuffer<LensDownsampleSlot> b_down_slots: register(t1);
StructuredBuffer<LensSlot> b_lens_slots: register(t2);
Texture2D<float4> t_lens_atlas: register(t3);
SamplerState s_lens_atlas: register(s1);

struct LensVertexOutput {
    nointerpolation uint lens_id: TEXCOORD0;
    nointerpolation uint slot_id: TEXCOORD1;
    float4 position: SV_Position;
    float4 clip_distance: SV_ClipDistance;
};

struct LensFragmentInput {
    nointerpolation uint lens_id: TEXCOORD0;
    nointerpolation uint slot_id: TEXCOORD1;
    float4 position: SV_Position;
};

LensVertexOutput lens_vertex(uint vertex_id: SV_VertexID, uint instance_id: SV_InstanceID) {
    float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
    uint lens_id = batch_start_index + instance_id;
    BackdropLens lens = b_lenses[lens_id];

    LensVertexOutput output;
    output.lens_id = lens_id;
    output.slot_id = instance_id;
    output.position = to_device_position(unit_vertex, lens.bounds);
    output.clip_distance = distance_from_clip_rect(unit_vertex, lens.bounds, lens.content_mask);
    return output;
}

// `smoothstep` as optics.rs has it: either edge may be the larger one.
float lens_step(float a, float b, float v) {
    float t = clamp((v - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

float lens_dim(float k) {
    return pow(clamp(k, 0.0, 1.0), 1.0 / 2.2);
}

float lens_round_rect(float2 p, float2 b, float r) {
    float2 q = abs(p) - b + r;
    return length(max(q, float2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

float3 lens_saturate(float3 c, float s) {
    if (abs(s - 1.0) < 1e-4) {
        return c;
    }
    float l = dot(c, float3(0.2126, 0.7152, 0.0722));
    return max(lerp(float3(l, l, l), c, s), float3(0.0, 0.0, 0.0));
}

// What the frame holds, as the sRGB-encoded colour the optics work in: as
// is in SDR, decoded back from scRGB (`to_output`) in HDR.
float3 lens_from_output(float3 color) {
    if (hdr_output == 0u) {
        return color;
    }
    return srgb_encode_exact(color / max(output_scale, 1e-4));
}

// Everything per lens the pixels share (optics.rs `Prepared`). Lengths are
// units.
struct LensShape {
    float2 center;     // device pixels
    float unit_dev;    // device pixels per unit
    float device_px;   // a device pixel, in units
    float css_px;      // a logical pixel, in units
    float2 half_size;
    float radius;
    float field_radius;
    float bevel;
    float band;
    float edge_band;
    float blur_shift;
    // QUILL (glass stage 5): gooey shapes, units from the body's centre (see
    // `lens_sdf`), and the frost's multiplier at the rim.
    float3 drop;       // x, radius, bridge softness; radius 0 = none
    float4 blob;       // centre x, y, half width, half height; 0 = none
    float2 blob_shape; // corner radius, bridge softness
    float2 blob_neck;  // x, falloff width; 0 = no neck
    float frost_edge;
};

LensShape lens_shape(BackdropLens lens) {
    LensShape s = (LensShape)0;
    float scale = max(lens.scale_factor, 1e-3);
    float unit = max(lens.unit, 1e-3);
    s.unit_dev = unit * scale;
    s.device_px = 1.0 / s.unit_dev;
    s.css_px = 1.0 / unit;
    s.center = lens.body.origin + lens.body.size * 0.5;
    float2 half_logical = lens.body.size * 0.5 / scale;
    s.half_size = max(half_logical / unit, float2(0.5, 0.5));
    float radius = min(lens.corner_radius / scale, min(half_logical.x - 0.5, half_logical.y - 0.5));
    s.radius = max(radius, 0.0) / unit;
    s.band = lens.material == 0u ? lens.band : 0.0;
    float min_half = max(min(half_logical.x, half_logical.y) - 0.5, 0.0) / unit;
    s.field_radius = min(max(s.radius, s.band), min_half);
    s.bevel = max(lens.bevel, 2.0);
    s.edge_band = 0.35 * max(lens.tune_bend_depth, 0.05);
    s.blur_shift = log2(max(s.unit_dev / 2.27, 1e-3));
    s.drop = float3(lens.drop_x, lens.drop_r, lens.drop_k) / unit;
    s.blob = float4(lens.blob_x, lens.blob_y, lens.blob_hx, lens.blob_hy) / unit;
    s.blob_shape = float2(lens.blob_r, lens.blob_k) / unit;
    s.blob_neck = float2(lens.neck_x, lens.neck_w) / unit;
    s.frost_edge = max(lens.tune_frost_edge, 0.05);
    return s;
}

// QUILL (glass stage 5): the source kit's smooth union (`melt`).
float lens_melt(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (a - b) / k, 0.0, 1.0);
    return lerp(a, b, h) - k * h * (1.0 - h);
}

// The blob's bridge softness at `p`: with a neck it peaks there and falls
// off, so a thinning bridge narrows into a thread before it breaks.
float lens_blob_soft(LensShape s, float2 p) {
    float k = s.blob_shape.y;
    if (s.blob_neck.y > 0.0) {
        float n = (p.x - s.blob_neck.x) / s.blob_neck.y;
        k *= exp(0.2 - sqrt(n * n + 0.04));
    }
    return max(k, 1e-3);
}

float lens_with_blob(LensShape s, float2 p, float body) {
    if (s.blob.z <= 0.0 || s.blob.w <= 0.0) {
        return body;
    }
    float blob = lens_round_rect(p - s.blob.xy, s.blob.zw, s.blob_shape.x);
    return lens_melt(blob, body, lens_blob_soft(s, p));
}

// The glass: the body, melted with its droplet and its blob.
float lens_sdf(LensShape s, float2 p) {
    float body = lens_round_rect(p, s.half_size, s.radius);
    if (s.drop.y > 0.0) {
        float d = length(p - float2(s.drop.x, 0.0)) - s.drop.y;
        body = lens_melt(d, body, max(s.drop.z, 1e-3));
    }
    return lens_with_blob(s, p, body);
}

// The body with corners as round as the bend is deep, and its blob (the
// droplet bends on its own rim only, as in the source kit).
float lens_field(LensShape s, float2 p) {
    return lens_with_blob(s, p, lens_round_rect(p, s.half_size, s.field_radius));
}

float lens_height(LensShape s, float2 p) {
    float t = pow(clamp(-lens_sdf(s, p) / s.bevel, 0.0, 1.0), 1.0 / 1.4);
    return sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
}

// What is behind the glass at `p` (units from the lens centre), blurred to
// mip level `lod`. Taps stay half a texel of that level inside the copied
// footprint, so the gutter between slots never shows.
float3 lens_back(LensShape s, LensSlot slot, float2 atlas_size, float2 p, float lod) {
    // A re-rendered slot holds `density` texels per device pixel: the same
    // blur is that many levels further down, on whole levels (its levels
    // below the sharpest are blurred, not just smaller).
    float density = max(slot.density, 1.0);
    float mip = clamp(lod + log2(density), 0.0, float(LENS_LEVELS - 1));
    if (density > 1.0) {
        mip = floor(mip);
    }
    float2 window = s.center + p * s.unit_dev;
    float inset = 0.5 * exp2(ceil(mip)) / density;
    float2 lo = float2(inset, inset);
    float2 hi = max(slot.copy_size - inset, lo);
    float2 local = clamp(window - slot.copy_origin, lo, hi);
    float2 uv = (slot.slot_origin + local * density) / atlas_size;
    return lens_from_output(t_lens_atlas.SampleLevel(s_lens_atlas, uv, mip).rgb);
}

// A gaussian of `sigma` device pixels, from the mip chain: level k holds
// about sigma 0.87 * 2^k, eight taps around the point widen it.
float3 lens_frosted(LensShape s, LensSlot slot, float2 atlas_size, float2 p, float sigma) {
    float lod = log2(max(sigma, 0.5) / 0.87);
    float mip = clamp(lod, 0.0, float(LENS_LEVELS - 1));
    float reach = 0.75 * exp2(mip) * max(exp2(lod - mip), 1.0) / s.unit_dev;
    float3 sum = float3(0.0, 0.0, 0.0);
    [unroll]
    for (int i = 0; i < 8; i++) {
        float angle = float(i) * (M_PI_F / 4.0) + 0.3;
        float r = (i % 2 == 0) ? reach : reach * 0.55;
        sum += lens_back(s, slot, atlas_size, p + float2(cos(angle), sin(angle)) * r, mip);
    }
    return sum / 8.0;
}

float3 lens_bent(LensShape s, LensSlot slot, float2 atlas_size, float2 p, float2 bend, float split, float lod) {
    float r = lens_back(s, slot, atlas_size, p + bend * (1.0 + split), lod).r;
    float g = lens_back(s, slot, atlas_size, p + bend, lod).g;
    float b = lens_back(s, slot, atlas_size, p + bend * (1.0 - split), lod).b;
    return float3(r, g, b);
}

float2 lens_radial_bend(LensShape s, BackdropLens lens, float2 q, float amount) {
    float d = lens_sdf(s, q);
    float len = max(length(q), 1e-4);
    float ratio = clamp(1.0 - abs(d) / (min(s.half_size.x, s.half_size.y) * s.edge_band / 0.35), 0.0, 1.0);
    float k = pow(ratio, lens.tune_bend_curve) * lens_step(1.5 * s.css_px, -0.5 * s.css_px, d);
    return -q / len * k * amount;
}

float4 lens_fragment(LensFragmentInput input): SV_Target {
    BackdropLens lens = b_lenses[input.lens_id];
    LensSlot slot = b_lens_slots[input.slot_id];
    uint atlas_w, atlas_h, atlas_levels;
    t_lens_atlas.GetDimensions(0, atlas_w, atlas_h, atlas_levels);
    float2 atlas_size = float2(float(atlas_w), float(atlas_h));
    LensShape s = lens_shape(lens);
    float strength = lens.strength;
    bool clear = lens.material == 0u;

    float2 local = (input.position.xy - s.center) / s.unit_dev;
    float dist = lens_sdf(s, local);

    // Drop shadow: tight + ambient, faded before the footprint's edge.
    float v_fade = lens_step(-0.1, 0.9, local.y / s.half_size.y) * 0.92 + 0.08;
    float outside = lens_step(-4.0, 14.0, dist);
    float soft = lens.shadow_softness;
    float off = lens.shadow_offset;
    float shadow_near = 1.0 - lens_step(-6.0, soft, lens_sdf(s, local - float2(0.0, off)));
    float shadow_far = 1.0 - lens_step(-10.0, soft * 1.5, lens_sdf(s, local - float2(0.0, off * 1.6)));
    float aa = max(s.device_px * 1.2, 1e-4);
    float mask = 1.0 - lens_step(-aa, aa, dist);
    float2 in_footprint = input.position.xy - lens.bounds.origin;
    float border = min(min(in_footprint.x, in_footprint.y),
                       min(lens.bounds.size.x - in_footprint.x, lens.bounds.size.y - in_footprint.y));
    float sh = clamp((shadow_near + shadow_far) * lens.shadow * strength * v_fade * outside, 0.0, 0.85)
        * lens_step(0.0, 14.0, border);
    float shadow_alpha = 1.0 - lens_dim(1.0 - sh);
    if (mask <= 0.0) {
        if (shadow_alpha <= 0.0) {
            discard;
        }
        return to_output(float4(0.0, 0.0, 0.0, shadow_alpha * lens.opacity));
    }

    // Surface normal from the bevel height field.
    float e = 1.25;
    float h = lens_height(s, local);
    float hx = lens_height(s, local + float2(e, 0.0));
    float hy = lens_height(s, local + float2(0.0, e));
    float3 n = float3(-(hx - h) / e * 42.0, -(hy - h) / e * 42.0, 1.0);
    float normal_z = n.z / length(n);
    float depth = clamp(1.0 - h, 0.0, 1.0);
    float edge = 1.0 - lens_step(0.0, s.bevel * s.edge_band, -dist);
    float2 dxy = float2(lens_sdf(s, local + float2(e, 0.0)) - dist,
                        lens_sdf(s, local + float2(0.0, e)) - dist);
    float2 dir = dxy / max(length(dxy), 1e-6);

    float4 color = hsla_to_rgba(lens.color);
    float4 surface = hsla_to_rgba(lens.surface);
    float frost_sigma = clear ? max(lens.blur, 0.0) * lens.scale_factor : 0.0;
    float3 glass;
    if (!clear) {
        glass = lerp(surface.rgb, color.rgb, 1.0 - edge * lens.edge_clear);
    } else if (lens.bend == 1u) {
        float amount = lens.radial_amount / max(lens.unit, 1e-3);
        float2 bend = lens_radial_bend(s, lens, local, amount);
        glass = lens_bent(s, slot, atlas_size, local, bend, 0.0, max(s.blur_shift, 0.0));
        if (frost_sigma > 0.0) {
            float ratio = clamp(1.0 - abs(dist) / (min(s.half_size.x, s.half_size.y) * s.edge_band / 0.35), 0.0, 1.0);
            float sigma = frost_sigma * lerp(1.0, s.frost_edge, pow(ratio, 1.3));
            float3 frost = lens_frosted(s, slot, atlas_size, local + bend, max(sigma, 0.5));
            glass = lerp(glass, frost, lens.blur_mix);
        }
        glass = lens_saturate(glass, lens.tune_saturation);
    } else {
        float bend_edge = edge;
        float2 bend_dir = dir;
        if (s.band > 0.0) {
            float fd = lens_field(s, local);
            float2 fxy = float2(lens_field(s, local + float2(e, 0.0)) - fd,
                                lens_field(s, local + float2(0.0, e)) - fd);
            bend_edge = 1.0 - lens_step(0.0, s.band, -fd);
            bend_dir = fxy / max(length(fxy), 1e-6);
        }
        float mag = lens.magnify * h + 1.0;
        float2 at = local / mag;
        float k = lens.refract * pow(bend_edge, lens.tune_bend_curve);
        float2 bend = -bend_dir * k;
        float split = lens.dispersion * 0.35;
        // QUILL (glass stage 5): the blur is measured where it shows, so a
        // magnified middle reads its backdrop that much finer.
        float lod = max(lerp(lens.blur_center, lens.blur_edge, pow(bend_edge, 1.3)) + s.blur_shift, 0.0)
            - log2(mag);
        glass = lens_bent(s, slot, atlas_size, at, bend, split, lod);
        if (frost_sigma > 0.0) {
            float sigma = frost_sigma * lerp(1.0, s.frost_edge, pow(bend_edge, 1.3));
            float3 frost = lens_frosted(s, slot, atlas_size, at + bend, max(sigma, 0.5));
            glass = lerp(glass, frost, lens.blur_mix);
        }
        glass = lens_saturate(glass, lens.tune_saturation);
    }

    // Top reflection.
    float ny = local.y / s.half_size.y;
    float nx = local.x / s.half_size.x;
    float top_refl = 0.3 * strength * lens.tune_reflection;
    float top_base = 0.02 * strength * lens.tune_reflection;
    float top = (1.0 - lens_step(-0.05, 0.75, ny)) * (1.0 - nx * 0.12) * top_refl * 0.6 + top_base;
    glass *= float3(lens_dim(1.0 - top), lens_dim(1.0 - top), lens_dim(1.0 - 0.72 * top));

    // Frosted resting state.
    if (clear) {
        glass = lerp(glass, color.rgb, lens.frost);
    }

    // Inner shading: just inside the rim, a wide band on the left, a deep ring.
    float inner = 0.1 * strength * lens.tune_inner;
    float up = clamp(-dir.y, 0.0, 1.0);
    float right = clamp(dir.x, 0.0, 1.0);
    float left = clamp(-dir.x, 0.0, 1.0);
    float down = clamp(dir.y, 0.0, 1.0);
    float inner_band = lens_step(s.bevel * -0.19, s.bevel * -0.024, dist);
    float k1 = lens_dim(1.0 - inner_band * (up + left * 0.6 + right * 0.3 + down * 0.15) * inner * 2.2);
    float deep_band = lens_step(s.bevel * -0.73, s.bevel * -0.13, dist);
    float k2 = lens_dim(1.0 - deep_band * left * inner * 2.0);
    float hxr = (dist + s.bevel * 0.45) / (s.bevel * 0.26);
    float ring = exp(-hxr * hxr);
    float k3 = lens_dim(1.0 - ring * (right * 0.9 + up * 0.5 + left * 0.4 + down * 0.3) * inner * 1.5);
    glass *= k1 * k2 * k3;

    // Rim light + fresnel, leaning towards the pointer.
    float rim = 0.55 * strength * lens.tune_rim;
    float fresnel = pow(clamp(1.0 - normal_z, 0.0, 1.0), 6.0);
    float rim_w = max(s.device_px * 1.1, 1.0) * max(lens.tune_rim_width, 0.05);
    float rim_line = 1.0 - lens_step(0.0, rim_w * 1.6, abs(dist + rim_w * 1.8));
    float fall = exp(-(local.y / 30.0) * (local.y / 30.0));
    float studio = right * right * fall * 0.9 + down * down * 0.2 + left * left * 0.55;
    float2 light = float2(lens.light_x, lens.light_y);
    float2 light_dir = length(light) > 1e-3 ? normalize(light) : float2(0.0, -1.0);
    float light_cos = dot(dir, light_dir);
    float facing = clamp(light_cos, 0.0, 1.0);
    float away = clamp(-light_cos, 0.0, 1.0);
    float lit_rim = facing * facing * 0.9 + away * away * 0.55;
    float lean = lens.light_mix * 0.4;
    float rim_dir = lerp(studio, lit_rim, lean);
    float front = lerp(right, facing, lean);
    float back_side = lerp(left, away, lean);
    float warm = 1.0 - lens_step(-0.5, 0.1, nx);
    float3 warm_cool = lerp(float3(0.82, 0.9, 1.08), float3(1.2, 0.95, 0.78), warm);
    float3 rim_col = lerp(float3(1.0, 1.0, 1.0), warm_cool, down * 0.6);
    rim_col = lerp(rim_col, float3(0.12, 0.6, 1.2), back_side * 0.85);
    float lit_on = rim > 0.001 ? 1.0 : 0.0;
    float add = rim_line * rim_dir * rim + fresnel * front * 0.02 * lens.tune_fresnel * lit_on;
    // Caustic: warm light gathering inside the bottom edge.
    float caustic = lens_step(0.72, 0.98, depth) * down * 0.08 * strength * lens.tune_caustic;
    glass += rim_col * add + float3(1.0, 0.92, 0.86) * caustic;

    float4 border_color = hsla_to_rgba(lens.border_color);
    float border_w = lens.border_width / max(lens.unit, 1e-3);
    if (border_w > 1e-4 && border_color.a > 0.0) {
        float t = lens_step(-border_w - aa, -border_w + aa, dist) * border_color.a;
        glass = lerp(glass, border_color.rgb, t);
    }

    // The glass over its shadow, straight alpha.
    float3 g = saturate(glass);
    float a = mask + shadow_alpha * (1.0 - mask);
    if (a <= 1e-5) {
        discard;
    }
    return to_output(float4(g * mask / max(a, 1e-5), a * lens.opacity));
}

struct LensDownsampleVertexOutput {
    nointerpolation uint slot_id: TEXCOORD0;
    float4 position: SV_Position;
};

LensDownsampleVertexOutput lens_downsample_vertex(uint vertex_id: SV_VertexID, uint instance_id: SV_InstanceID) {
    float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
    uint slot_id = batch_start_index + instance_id;
    LensDownsampleSlot slot = b_down_slots[slot_id];
    // The level written is half the level read (the atlas's sides are
    // multiples of 256, so every level halves exactly).
    uint source_w, source_h, source_levels;
    t_lens_atlas.GetDimensions(0, source_w, source_h, source_levels);
    float2 target_size = float2(float(source_w), float(source_h)) * 0.5;
    float2 position = slot.dst_origin + unit_vertex * slot.dst_size;

    LensDownsampleVertexOutput output;
    output.slot_id = slot_id;
    output.position = float4(position / target_size * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    return output;
}

// Dual-Kawase downsample: the centre and four diagonal taps one source
// texel away, each a bilinear average of four source texels, kept inside
// the slot's part of the source level (bound alone at t3).
float4 lens_downsample_fragment(LensDownsampleVertexOutput input): SV_Target {
    LensDownsampleSlot slot = b_down_slots[input.slot_id];
    uint source_w, source_h, source_levels;
    t_lens_atlas.GetDimensions(0, source_w, source_h, source_levels);
    float2 source_size = float2(float(source_w), float(source_h));
    float2 lo = slot.src_min + 0.5;
    float2 hi = max(slot.src_max - 0.5, lo);
    float2 c = input.position.xy * 2.0;
    float4 sum = t_lens_atlas.SampleLevel(s_lens_atlas, clamp(c, lo, hi) / source_size, 0.0) * 4.0;
    sum += t_lens_atlas.SampleLevel(s_lens_atlas, clamp(c + float2(1.0, 1.0), lo, hi) / source_size, 0.0);
    sum += t_lens_atlas.SampleLevel(s_lens_atlas, clamp(c + float2(-1.0, 1.0), lo, hi) / source_size, 0.0);
    sum += t_lens_atlas.SampleLevel(s_lens_atlas, clamp(c + float2(1.0, -1.0), lo, hi) / source_size, 0.0);
    sum += t_lens_atlas.SampleLevel(s_lens_atlas, clamp(c + float2(-1.0, -1.0), lo, hi) / source_size, 0.0);
    return sum / 8.0;
}

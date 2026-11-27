#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

constant uint SIMPLE_PRIMITIVE_FLAG = 1u << 31;

// QUILL: instance layouts, in bytes: each struct's size and the offset of
// its `content_mask`. src/ui/rounded_mask_tests.rs checks these lines
// against the Rust structs, so a struct changed on one side fails a test.
// QUILL-LAYOUT Quad size=192 mask=24
// QUILL-LAYOUT Shadow size=144 mask=40
// QUILL-LAYOUT Underline size=96 mask=24
// QUILL-LAYOUT MonochromeSprite size=144 mask=56
// QUILL-LAYOUT PolychromeSprite size=128 mask=64
// QUILL-LAYOUT PathPaint size=144 mask=16
// QUILL-LAYOUT BackdropLens size=328 mask=24

float4 hsla_to_rgba(Hsla hsla);
float3 srgb_to_linear(float3 color);
float3 linear_to_srgb(float3 color);
float4 srgb_to_oklab(float4 color);
float4 oklab_to_srgb(float4 color);
float4 to_device_position_impl(float2 position,
                               constant Size_DevicePixels *viewport_size);
float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *viewport_size);
float4 to_device_position_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          TransformationMatrix transformation,
                          constant Size_DevicePixels *input_viewport_size);

float2 to_tile_position(float2 unit_vertex, AtlasTile tile,
                        constant Size_DevicePixels *atlas_size);
float4 distance_from_clip_rect_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds, TransformationMatrix transformation);

struct ClippedVertex {
  float2 position;
  // The corner as a fraction of the original bounds.
  float2 unit_vertex;
};

ClippedVertex clip_to_mask(float2 unit_vertex, Bounds_ScaledPixels bounds,
                           Bounds_ScaledPixels mask);
float corner_dash_velocity(float dv1, float dv2);
float dash_alpha(float t, float period, float length, float dash_velocity,
                 float antialias_threshold);
float quarter_ellipse_sdf(float2 point, float2 radii);
float pick_corner_radius(float2 center_to_point, Corners_ScaledPixels corner_radii);
float quad_sdf(float2 point, Bounds_ScaledPixels bounds,
               Corners_ScaledPixels corner_radii);
float quad_sdf_impl(float2 center_to_point, float corner_radius);
float4 over(float4 below, float4 above);
float radians(float degrees);

struct PreparedBackground {
  float4 color0 [[flat]];
  float4 color1 [[flat]];
  uint tag [[flat]];
};

PreparedBackground prepare_background(Background background);
float dither_offset(float2 position);
float4 background_color(PreparedBackground prepared, Background background,
                        Bounds_ScaledPixels bounds, float2 position);
bool has_rounded_corners(Corners_ScaledPixels corner_radii);
bool has_border(Edges_ScaledPixels border_widths);
bool is_shaped(Quad quad);
float rounded_mask_coverage(float2 position, RoundedMask_ScaledPixels mask);
bool has_rounded_mask(ContentMask_ScaledPixels mask);

struct QuadFragmentInput {
  uint quad_id [[flat]];
  float4 position [[position]];
  float4 border_color [[flat]];
  PreparedBackground background;
};

constant float4 CULLED_POSITION = float4(0., 0., 0., 1.);

float quad_depth(uint quad_id, float depth_scale) {
  return saturate(float(quad_id + 1u) * depth_scale);
}

// Opaque rectangles are drawn by the opaque pass instead of the quad pass.
bool is_opaque_rectangle(Quad quad) {
  return quad.background.tag == 0u && quad.background.solid.a >= 1.0 &&
         !has_rounded_corners(quad.corner_radii) &&
         !has_border(quad.border_widths) &&
         !has_rounded_mask(quad.content_mask);
}

vertex QuadFragmentInput quad_vertex(
    uint vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]],
    constant float *depth_scale [[buffer(GlobalInputIndex_DepthScale)]]) {
  Quad quad = quads[quad_id];
  // QUILL: a zero depth scale is a frame drawn without the opaque pre-pass
  // (a frame with glass), so this pass draws every quad.
  if (*depth_scale > 0.0 && is_opaque_rectangle(quad)) {
    QuadFragmentInput culled = {};
    culled.position = CULLED_POSITION;
    return culled;
  }

  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  ClippedVertex clipped =
      clip_to_mask(unit_vertex, quad.bounds, quad.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  device_position.z = quad_depth(quad_id, *depth_scale);
  float4 border_color = hsla_to_rgba(quad.border_color);
  bool simple = !is_shaped(quad) && quad.background.tag == 0u &&
                !has_rounded_mask(quad.content_mask);
  PreparedBackground background = prepare_background(quad.background);

  return QuadFragmentInput{
      quad_id | (simple ? SIMPLE_PRIMITIVE_FLAG : 0u),
      device_position,
      border_color,
      background};
}

fragment float4 quad_fragment(QuadFragmentInput input [[stage_in]],
                              constant Quad *quads
                              [[buffer(QuadInputIndex_Quads)]]) {
  if ((input.quad_id & SIMPLE_PRIMITIVE_FLAG) != 0u) {
    return input.background.color0;
  }

  Quad quad = quads[input.quad_id];
  float mask_coverage =
      rounded_mask_coverage(input.position.xy, quad.content_mask.rounded);
  float4 background = background_color(input.background, quad.background,
                                       quad.bounds, input.position.xy);
  bool unrounded = !has_rounded_corners(quad.corner_radii);

  float2 size = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 half_size = size / 2.0;
  float2 point = input.position.xy - float2(quad.bounds.origin.x, quad.bounds.origin.y);
  float2 center_to_point = point - half_size;

  // Signed distance field threshold for inclusion of pixels. 0.5 is the
  // minimum distance between the center of the pixel and the edge.
  const float antialias_threshold = 0.5;

  // Radius of the nearest corner
  float corner_radius = pick_corner_radius(center_to_point, quad.corner_radii);

  // Width of the nearest borders
  float2 border = float2(
    center_to_point.x < 0.0 ? quad.border_widths.left : quad.border_widths.right,
    center_to_point.y < 0.0 ? quad.border_widths.top : quad.border_widths.bottom
  );

  // 0-width borders are reduced so that `inner_sdf >= antialias_threshold`.
  // The purpose of this is to not draw antialiasing pixels in this case.
  float2 reduced_border = float2(
    border.x == 0.0 ? -antialias_threshold : border.x,
    border.y == 0.0 ? -antialias_threshold : border.y);

  // Vector from the corner of the quad bounds to the point, after mirroring
  // the point into the bottom right quadrant. Both components are <= 0.
  float2 corner_to_point = fabs(center_to_point) - half_size;

  // Vector from the point to the center of the rounded corner's circle, also
  // mirrored into bottom right quadrant.
  float2 corner_center_to_point = corner_to_point + corner_radius;

  // Whether the nearest point on the border is rounded
  bool is_near_rounded_corner =
    corner_center_to_point.x >= 0.0 &&
    corner_center_to_point.y >= 0.0;

  // Vector from straight border inner corner to point.
  //
  // 0-width borders are turned into width -1 so that inner_sdf is > 1.0 near
  // the border. Without this, antialiasing pixels would be drawn.
  float2 straight_border_inner_corner_to_point = corner_to_point + reduced_border;

  // Whether the point is beyond the inner edge of the straight border
  bool is_beyond_inner_straight_border =
    straight_border_inner_corner_to_point.x > 0.0 ||
    straight_border_inner_corner_to_point.y > 0.0;


  // Whether the point is far enough inside the quad, such that the pixels are
  // not affected by the straight border.
  bool is_within_inner_straight_border =
    straight_border_inner_corner_to_point.x < -antialias_threshold &&
    straight_border_inner_corner_to_point.y < -antialias_threshold;

  // Fast path for points that must be part of the background
  if (is_within_inner_straight_border && !is_near_rounded_corner) {
    return background * float4(1.0, 1.0, 1.0, mask_coverage);
  }

  // Signed distance of the point to the outside edge of the quad's border
  float outer_sdf = quad_sdf_impl(corner_center_to_point, corner_radius);

  // Approximate signed distance of the point to the inside edge of the quad's
  // border. It is negative outside this edge (within the border), and
  // positive inside.
  //
  // This is not always an accurate signed distance:
  // * The rounded portions with varying border width use an approximation of
  //   nearest-point-on-ellipse.
  // * When it is quickly known to be outside the edge, -1.0 is used.
  float inner_sdf = 0.0;
  if (corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0) {
    // Fast paths for straight borders
    inner_sdf = -max(straight_border_inner_corner_to_point.x,
                     straight_border_inner_corner_to_point.y);
  } else if (is_beyond_inner_straight_border) {
    // Fast path for points that must be outside the inner edge
    inner_sdf = -1.0;
  } else if (reduced_border.x == reduced_border.y) {
    // Fast path for circular inner edge.
    inner_sdf = -(outer_sdf + reduced_border.x);
  } else {
    float2 ellipse_radii = max(float2(0.0), float2(corner_radius) - reduced_border);
    inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
  }

  // Negative when inside the border
  float border_sdf = max(inner_sdf, outer_sdf);

  float4 color = background;
  if (border_sdf < antialias_threshold) {
    float4 border_color = input.border_color;

    // Dashed border logic when border_style == 1
    if (quad.border_style == 1) {
      // Position along the perimeter in "dash space", where each dash
      // period has length 1
      float t = 0.0;

      // Total number of dash periods, so that the dash spacing can be
      // adjusted to evenly divide it
      float max_t = 0.0;

      // Border width is proportional to dash size. This is the behavior
      // used by browsers, but also avoids dashes from different segments
      // overlapping when dash size is smaller than the border width.
      //
      // Dash pattern: (2 * border width) dash, (1 * border width) gap
      const float dash_length_per_width = 2.0;
      const float dash_gap_per_width = 1.0;
      const float dash_period_per_width = dash_length_per_width + dash_gap_per_width;

      // Since the dash size is determined by border width, the density of
      // dashes varies. Multiplying a pixel distance by this returns a
      // position in dash space - it has units (dash period / pixels). So
      // a dash velocity of (1 / 10) is 1 dash every 10 pixels.
      float dash_velocity = 0.0;

      // Dividing this by the border width gives the dash velocity
      const float dv_numerator = 1.0 / dash_period_per_width;

      if (unrounded) {
        // When corners aren't rounded, the dashes are separately laid
        // out on each straight line, rather than around the whole
        // perimeter. This way each line starts and ends with a dash.
        bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;

        // Choosing the right border width for dashed borders.
        // TODO: A better solution exists taking a look at the whole file.
        // this does not fix single dashed borders at the corners
        float2 dashed_border = float2(
        fmax(quad.border_widths.bottom, quad.border_widths.top),
        fmax(quad.border_widths.right, quad.border_widths.left));

        float border_width = is_horizontal ? dashed_border.x : dashed_border.y;
        dash_velocity = dv_numerator / border_width;
        t = is_horizontal ? point.x : point.y;
        t *= dash_velocity;
        max_t = is_horizontal ? size.x : size.y;
        max_t *= dash_velocity;
      } else {
        // When corners are rounded, the dashes are laid out clockwise
        // around the whole perimeter.

        float r_tr = quad.corner_radii.top_right;
        float r_br = quad.corner_radii.bottom_right;
        float r_bl = quad.corner_radii.bottom_left;
        float r_tl = quad.corner_radii.top_left;

        float w_t = quad.border_widths.top;
        float w_r = quad.border_widths.right;
        float w_b = quad.border_widths.bottom;
        float w_l = quad.border_widths.left;

        // Straight side dash velocities
        float dv_t = w_t <= 0.0 ? 0.0 : dv_numerator / w_t;
        float dv_r = w_r <= 0.0 ? 0.0 : dv_numerator / w_r;
        float dv_b = w_b <= 0.0 ? 0.0 : dv_numerator / w_b;
        float dv_l = w_l <= 0.0 ? 0.0 : dv_numerator / w_l;

        // Straight side lengths in dash space
        float s_t = (size.x - r_tl - r_tr) * dv_t;
        float s_r = (size.y - r_tr - r_br) * dv_r;
        float s_b = (size.x - r_br - r_bl) * dv_b;
        float s_l = (size.y - r_bl - r_tl) * dv_l;

        float corner_dash_velocity_tr = corner_dash_velocity(dv_t, dv_r);
        float corner_dash_velocity_br = corner_dash_velocity(dv_b, dv_r);
        float corner_dash_velocity_bl = corner_dash_velocity(dv_b, dv_l);
        float corner_dash_velocity_tl = corner_dash_velocity(dv_t, dv_l);

        // Corner lengths in dash space
        float c_tr = r_tr * (M_PI_F / 2.0) * corner_dash_velocity_tr;
        float c_br = r_br * (M_PI_F / 2.0) * corner_dash_velocity_br;
        float c_bl = r_bl * (M_PI_F / 2.0) * corner_dash_velocity_bl;
        float c_tl = r_tl * (M_PI_F / 2.0) * corner_dash_velocity_tl;

        // Cumulative dash space upto each segment
        float upto_tr = s_t;
        float upto_r = upto_tr + c_tr;
        float upto_br = upto_r + s_r;
        float upto_b = upto_br + c_br;
        float upto_bl = upto_b + s_b;
        float upto_l = upto_bl + c_bl;
        float upto_tl = upto_l + s_l;
        max_t = upto_tl + c_tl;

        if (is_near_rounded_corner) {
          float radians = atan2(corner_center_to_point.y, corner_center_to_point.x);
          float corner_t = radians * corner_radius;

          if (center_to_point.x >= 0.0) {
            if (center_to_point.y < 0.0) {
              dash_velocity = corner_dash_velocity_tr;
              // Subtracted because radians is pi/2 to 0 when
              // going clockwise around the top right corner,
              // since the y axis has been flipped
              t = upto_r - corner_t * dash_velocity;
            } else {
              dash_velocity = corner_dash_velocity_br;
              // Added because radians is 0 to pi/2 when going
              // clockwise around the bottom-right corner
              t = upto_br + corner_t * dash_velocity;
            }
          } else {
            if (center_to_point.y >= 0.0) {
              dash_velocity = corner_dash_velocity_bl;
              // Subtracted because radians is pi/1 to 0 when
              // going clockwise around the bottom-left corner,
              // since the x axis has been flipped
              t = upto_l - corner_t * dash_velocity;
            } else {
              dash_velocity = corner_dash_velocity_tl;
              // Added because radians is 0 to pi/2 when going
              // clockwise around the top-left corner, since both
              // axis were flipped
              t = upto_tl + corner_t * dash_velocity;
            }
          }
        } else {
          // Straight borders
          bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
          if (is_horizontal) {
            if (center_to_point.y < 0.0) {
              dash_velocity = dv_t;
              t = (point.x - r_tl) * dash_velocity;
            } else {
              dash_velocity = dv_b;
              t = upto_bl - (point.x - r_bl) * dash_velocity;
            }
          } else {
            if (center_to_point.x < 0.0) {
              dash_velocity = dv_l;
              t = upto_tl - (point.y - r_tl) * dash_velocity;
            } else {
              dash_velocity = dv_r;
              t = upto_r + (point.y - r_tr) * dash_velocity;
            }
          }
        }
      }

      float dash_length = dash_length_per_width / dash_period_per_width;
      float desired_dash_gap = dash_gap_per_width / dash_period_per_width;

      // Straight borders should start and end with a dash, so max_t is
      // reduced to cause this.
      max_t -= unrounded ? dash_length : 0.0;
      if (max_t >= 1.0) {
        // Adjust dash gap to evenly divide max_t
        float dash_count = floor(max_t);
        float dash_period = max_t / dash_count;
        border_color.a *= dash_alpha(t, dash_period, dash_length, dash_velocity,
                                     antialias_threshold);
      } else if (unrounded) {
        // When there isn't enough space for the full gap between the
        // two start / end dashes of a straight border, reduce gap to
        // make them fit.
        float dash_gap = max_t - dash_length;
        if (dash_gap > 0.0) {
          float dash_period = dash_length + dash_gap;
          border_color.a *= dash_alpha(t, dash_period, dash_length, dash_velocity,
                                       antialias_threshold);
        }
      }
    }

    border_color.a *= saturate(antialias_threshold - inner_sdf);
    color = over(background, border_color);
  }

  return color * float4(1.0, 1.0, 1.0,
                        saturate(antialias_threshold - outer_sdf) * mask_coverage);
}

// Returns the dash velocity of a corner given the dash velocity of the two
// sides, by returning the slower velocity (larger dashes).
//
// Since 0 is used for dash velocity when the border width is 0 (instead of
// +inf), this returns the other dash velocity in that case.
//
// An alternative to this might be to appropriately interpolate the dash
// velocity around the corner, but that seems overcomplicated.
float corner_dash_velocity(float dv1, float dv2) {
  if (dv1 == 0.0) {
    return dv2;
  } else if (dv2 == 0.0) {
    return dv1;
  } else {
    return min(dv1, dv2);
  }
}

// Returns alpha used to render antialiased dashes.
// `t` is within the dash when `fmod(t, period) < length`.
float dash_alpha(
    float t, float period, float length, float dash_velocity,
    float antialias_threshold) {
  float half_period = period / 2.0;
  float half_length = length / 2.0;
  // Value in [-half_period, half_period]
  // The dash is in [-half_length, half_length]
  float centered = fmod(t + half_period - half_length, period) - half_period;
  // Signed distance for the dash, negative values are inside the dash
  float signed_distance = abs(centered) - half_length;
  // Antialiased alpha based on the signed distance
  return saturate(antialias_threshold - signed_distance / dash_velocity);
}

// This approximates distance to the nearest point to a quarter ellipse in a way
// that is sufficient for anti-aliasing when the ellipse is not very eccentric.
// The components of `point` are expected to be positive.
//
// Negative on the outside and positive on the inside.
float quarter_ellipse_sdf(float2 point, float2 radii) {
  // Scale the space to treat the ellipse like a unit circle
  float2 circle_vec = point / radii;
  float unit_circle_sdf = length(circle_vec) - 1.0;
  // Approximate up-scaling of the length by using the average of the radii.
  //
  // TODO: A better solution would be to use the gradient of the implicit
  // function for an ellipse to approximate a scaling factor.
  return unit_circle_sdf * (radii.x + radii.y) * -0.5;
}

struct OpaqueQuadFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
};

vertex OpaqueQuadFragmentInput opaque_quad_vertex(
    uint vertex_id [[vertex_id]], uint instance_id [[instance_id]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant uint *quad_count [[buffer(QuadInputIndex_QuadCount)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]],
    constant float *depth_scale [[buffer(GlobalInputIndex_DepthScale)]]) {
  uint quad_id = *quad_count - 1u - instance_id;
  Quad quad = quads[quad_id];
  if (!is_opaque_rectangle(quad)) {
    return OpaqueQuadFragmentInput{CULLED_POSITION, float4(0.)};
  }

  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  ClippedVertex clipped =
      clip_to_mask(unit_vertex, quad.bounds, quad.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  device_position.z = quad_depth(quad_id, *depth_scale);
  return OpaqueQuadFragmentInput{device_position,
                                 hsla_to_rgba(quad.background.solid)};
}

fragment float4 opaque_quad_fragment(OpaqueQuadFragmentInput input
                                     [[stage_in]]) {
  return input.color;
}

struct PreparedBlurredRoundedRect {
  float4 shape;
  float4 r1;
  float4 exponent;
  float4 scale;
};

struct ShadowFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
  uint shadow_id [[flat]];
  float4 blur_shape [[flat]];
  float4 blur_r1 [[flat]];
  float4 blur_exponent [[flat]];
  float4 blur_scale [[flat]];
};

float erf7(float x) {
  x = clamp(x, -10., 10.) * (2. / sqrt(M_PI_F));
  float xx = x * x;
  x = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
  return x / sqrt(1. + x * x);
}

// Raph Levien's blurred rounded rectangle approximation, whose blur s is sigma * sqrt(2):
// https://raphlinus.github.io/graphics/2020/04/21/blurred-rounded-rects.html
PreparedBlurredRoundedRect prepare_blurred_rounded_rect(float2 size, Corners_ScaledPixels corner_radii, float sigma) {
  float s = max(sigma * sqrt(2.), 1e-6);
  float s_inv = 1. / s;
  float min_edge = min(size.x, size.y);
  float r_max = 0.5 * min_edge;
  float4 radii = float4(corner_radii.top_left, corner_radii.top_right,
                        corner_radii.bottom_right, corner_radii.bottom_left);
  float4 r0 = min(sqrt(radii * radii + 1.15 * 1.15 * s * s), r_max);
  float4 r1 = min(sqrt(radii * radii + 4. * s * s), r_max);

  float2 half_extent = 0.5 * s_inv * size;
  float delta = 1.25 * s * (exp(-half_extent.x * half_extent.x) - exp(-half_extent.y * half_extent.y));
  float2 adjusted_size = float2(size.x + min(delta, 0.), size.y - max(delta, 0.));
  float4 peak = s_inv * 0.5 * (max(adjusted_size.x, adjusted_size.y) - 0.5 * radii);

  PreparedBlurredRoundedRect output;
  output.shape = float4(0.5 * adjusted_size, s_inv, min_edge);
  output.r1 = r1;
  output.exponent = 2. * r1 / r0;
  output.scale = 0.5 * float4(erf7(peak.x), erf7(peak.y), erf7(peak.z), erf7(peak.w));
  return output;
}

float pick_corner(float2 center_to_point, float4 values) {
  Corners_ScaledPixels corners = {values.x, values.y, values.z, values.w};
  return pick_corner_radius(center_to_point, corners);
}

float blurred_rounded_rect(float2 center_to_point, PreparedBlurredRoundedRect prepared) {
  float r1 = pick_corner(center_to_point, prepared.r1);
  float exponent = pick_corner(center_to_point, prepared.exponent);
  float scale = pick_corner(center_to_point, prepared.scale);
  float s_inv = prepared.shape.z;
  float min_edge = prepared.shape.w;

  float2 p0 = abs(center_to_point) - (prepared.shape.xy - r1);
  float2 p1 = max(p0, 0.);
  float d_pos;
  if (p1.x > 0. && p1.y > 0.) {
    d_pos = pow(pow(p1.x, exponent) + pow(p1.y, exponent), 1. / exponent);
  } else {
    d_pos = p1.x + p1.y;
  }

  float d_neg = min(max(p0.x, p0.y), 0.);
  float d = d_pos + d_neg - r1;
  return scale * (erf7(s_inv * (min_edge + d)) - erf7(s_inv * d));
}

vertex ShadowFragmentInput shadow_vertex(
    uint vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  Shadow shadow = shadows[shadow_id];

  ClippedVertex clipped =
      clip_to_mask(unit_vertex, shadow.bounds, shadow.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  float4 color = hsla_to_rgba(shadow.color);
  PreparedBlurredRoundedRect blur = prepare_blurred_rounded_rect(
      float2(shadow.shape_bounds.size.width, shadow.shape_bounds.size.height),
      shadow.shape_corner_radii, shadow.blur_radius);

  return ShadowFragmentInput{
      device_position,
      color,
      shadow_id,
      blur.shape,
      blur.r1,
      blur.exponent,
      blur.scale};
}

fragment float4 shadow_fragment(ShadowFragmentInput input [[stage_in]],
                                constant Shadow *shadows
                                [[buffer(ShadowInputIndex_Shadows)]]) {
  Shadow shadow = shadows[input.shadow_id];

  float alpha;
  if (shadow.blur_radius == 0.) {
    float distance = quad_sdf(input.position.xy, shadow.shape_bounds,
                              shadow.shape_corner_radii);
    alpha = saturate(0.5 - distance);
  } else {
    float2 origin = float2(shadow.shape_bounds.origin.x, shadow.shape_bounds.origin.y);
    float2 size = float2(shadow.shape_bounds.size.width, shadow.shape_bounds.size.height);
    float2 center = origin + size / 2.;
    PreparedBlurredRoundedRect blur = {input.blur_shape, input.blur_r1,
                                       input.blur_exponent, input.blur_scale};
    alpha = blurred_rounded_rect(input.position.xy - center, blur);
  }

  if (shadow.inset != 0u) {
    // The inset shadow is the complement of the (blurred) hole rect, clipped to the element.
    // `saturate(0.5 - d)` gives a 1-pixel antialiased edge: d <= -0.5 -> 1, d >= 0.5 -> 0.
    alpha = 1. - alpha;
    float element_distance =
        quad_sdf(input.position.xy, shadow.bounds, shadow.corner_radii);
    alpha *= saturate(0.5 - element_distance);
  }

  alpha *= rounded_mask_coverage(input.position.xy, shadow.content_mask.rounded);
  return input.color * float4(1., 1., 1., alpha);
}

struct UnderlineFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
  uint underline_id [[flat]];
};

vertex UnderlineFragmentInput underline_vertex(
    uint vertex_id [[vertex_id]], uint underline_id [[instance_id]],
    constant Underline *underlines [[buffer(UnderlineInputIndex_Underlines)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  Underline underline = underlines[underline_id];
  ClippedVertex clipped = clip_to_mask(unit_vertex, underline.bounds,
                                       underline.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  float4 color = hsla_to_rgba(underline.color);
  return UnderlineFragmentInput{
      device_position,
      color,
      underline_id | (underline.wavy == 0u && !has_rounded_mask(underline.content_mask)
                          ? SIMPLE_PRIMITIVE_FLAG
                          : 0u)};
}

fragment float4 underline_fragment(UnderlineFragmentInput input [[stage_in]],
                                   constant Underline *underlines
                                   [[buffer(UnderlineInputIndex_Underlines)]]) {
  const float WAVE_FREQUENCY = 2.0;
  const float WAVE_HEIGHT_RATIO = 0.8;

  if ((input.underline_id & SIMPLE_PRIMITIVE_FLAG) != 0u) {
    return input.color;
  }

  Underline underline = underlines[input.underline_id];
  float half_thickness = underline.thickness * 0.5;
  float2 origin =
      float2(underline.bounds.origin.x, underline.bounds.origin.y);

  float2 st = ((input.position.xy - origin) / underline.bounds.size.height) -
              float2(0., 0.5);
  float frequency = (M_PI_F * WAVE_FREQUENCY * underline.thickness) / underline.bounds.size.height;
  float amplitude = (underline.thickness * WAVE_HEIGHT_RATIO) / underline.bounds.size.height;

  float sine = sin(st.x * frequency) * amplitude;
  float dSine = cos(st.x * frequency) * amplitude * frequency;
  float distance = (st.y - sine) / sqrt(1. + dSine * dSine);
  float distance_in_pixels = distance * underline.bounds.size.height;
  float distance_from_top_border = distance_in_pixels - half_thickness;
  float distance_from_bottom_border = distance_in_pixels + half_thickness;
  float alpha = saturate(
      0.5 - max(-distance_from_bottom_border, distance_from_top_border));
  if (underline.wavy == 0u) {
    alpha = 1.0;
  }
  alpha *= rounded_mask_coverage(input.position.xy, underline.content_mask.rounded);
  return input.color * float4(1., 1., 1., alpha);
}

struct MonochromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  float4 color [[flat]];
  float4 clip_distance;
  uint sprite_id [[flat]];
  // QUILL: 1 when the sprite's mask has a rounded shape, so the fragment
  // reads the instance only then (docs/decisions/rounded-content-masks.md).
  uint rounded [[flat]];
};

vertex MonochromeSpriteFragmentInput monochrome_sprite_vertex(
    uint vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant MonochromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]],
    constant LensInkInstance *inks [[buffer(SpriteInputIndex_Inks)]],
    texture2d<float> lens_atlas [[texture(SpriteInputIndex_LensAtlas)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  MonochromeSprite sprite = sprites[sprite_id];
  float4 device_position =
      to_device_position_transformed(unit_vertex, sprite.bounds, sprite.transformation, viewport_size);
  float4 clip_distance = distance_from_clip_rect_transformed(unit_vertex, sprite.bounds,
                                                 sprite.content_mask.bounds, sprite.transformation);
  float2 tile_position = to_tile_position(unit_vertex, sprite.tile, atlas_size);
  float4 color = hsla_to_rgba(sprite.color);
  // QUILL (glass stage 5): an inked label takes the ink that reads on the glass
  // under its centre, judged from the glass's colour over the backdrop the
  // lens bends (the lens atlas, blurred), in this frame.
  if (sprite.animation > 0) {
    LensInkInstance ink = inks[sprite.animation - 1];
    if (ink.active > 0.5) {
      float luma = ink.glass_luma;
      if (ink.sampled > 0.5) {
        constexpr sampler ink_sampler(mag_filter::linear, min_filter::linear,
                                      mip_filter::linear,
                                      address::clamp_to_edge);
        float2 centre =
            float2(sprite.bounds.origin.x + sprite.bounds.size.width * 0.5,
                   sprite.bounds.origin.y + sprite.bounds.size.height * 0.5);
        float2 copy_size = float2(ink.copy_w, ink.copy_h);
        float2 local =
            clamp(centre - float2(ink.copy_x, ink.copy_y), float2(8.0),
                  max(copy_size - 8.0, float2(8.0)));
        float2 lens_atlas_size =
            float2(lens_atlas.get_width(), lens_atlas.get_height());
        float2 uv = (float2(ink.slot_x, ink.slot_y) + local * ink.density) /
                    lens_atlas_size;
        float lod = min(4.0 + log2(max(ink.density, 1.0)),
                        float(lens_atlas.get_num_mip_levels() - 1));
        float3 back = lens_atlas.sample(ink_sampler, uv, level(lod)).rgb;
        float back_luma = dot(back, float3(0.2126, 0.7152, 0.0722));
        luma = mix(back_luma, ink.glass_luma, clamp(ink.cover, 0.0, 1.0));
      }
      float light = smoothstep(0.44, 0.56, luma);
      // The ink's colour at the label's own alpha (its opacity included).
      float alpha = color.a;
      color = mix(hsla_to_rgba(ink.on_dark), hsla_to_rgba(ink.on_light), light);
      color.a = alpha;
    }
  }
  return MonochromeSpriteFragmentInput{
      device_position,
      tile_position,
      color,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w},
      sprite_id,
      has_rounded_mask(sprite.content_mask) ? 1u : 0u};
}

fragment float4 monochrome_sprite_fragment(
    MonochromeSpriteFragmentInput input [[stage_in]],
    constant MonochromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    texture2d<float> atlas_texture [[texture(SpriteInputIndex_AtlasTexture)]]) {
  if (any(input.clip_distance < float4(0.0))) {
    return float4(0.0);
  }

  constexpr sampler atlas_texture_sampler(mag_filter::linear,
                                          min_filter::linear);
  float4 sample =
      atlas_texture.sample(atlas_texture_sampler, input.tile_position);
  float4 color = input.color;
  color.a *= sample.a;
  if (input.rounded != 0u) {
    color.a *= rounded_mask_coverage(
        input.position.xy, sprites[input.sprite_id].content_mask.rounded);
  }
  return color;
}

struct PolychromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  uint sprite_id [[flat]];
  float opacity [[flat]];
};

vertex PolychromeSpriteFragmentInput polychrome_sprite_vertex(
    uint vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  PolychromeSprite sprite = sprites[sprite_id];
  ClippedVertex clipped =
      clip_to_mask(unit_vertex, sprite.bounds, sprite.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  float2 tile_position =
      to_tile_position(clipped.unit_vertex, sprite.tile, atlas_size);
  // QUILL: `flags` (`SpriteFlags`): 1 grayscale, 2 premultiplied (a
  // snapshot's texture); either takes the full path.
  bool simple = !has_rounded_corners(sprite.corner_radii) &&
                sprite.flags == 0u &&
                !has_rounded_mask(sprite.content_mask);
  return PolychromeSpriteFragmentInput{
      device_position,
      tile_position,
      sprite_id | (simple ? SIMPLE_PRIMITIVE_FLAG : 0u),
      sprite.opacity};
}

fragment float4 polychrome_sprite_fragment(
    PolychromeSpriteFragmentInput input [[stage_in]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    texture2d<float> atlas_texture [[texture(SpriteInputIndex_AtlasTexture)]]) {
  constexpr sampler atlas_texture_sampler(mag_filter::linear,
                                          min_filter::linear);
  float4 color =
      atlas_texture.sample(atlas_texture_sampler, input.tile_position);
  if ((input.sprite_id & SIMPLE_PRIMITIVE_FLAG) != 0u) {
    return float4(color.rgb, color.a * input.opacity);
  }

  PolychromeSprite sprite = sprites[input.sprite_id];
  // QUILL: a snapshot's texels are premultiplied; the blend wants them
  // straight.
  if ((sprite.flags & 2u) != 0u && color.a > 0.0) {
    color.rgb /= color.a;
  }
  if ((sprite.flags & 1u) != 0u) {
    float grayscale = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
    color.r = grayscale;
    color.g = grayscale;
    color.b = grayscale;
  }
  float distance =
      quad_sdf(input.position.xy, sprite.bounds, sprite.corner_radii);
  color.a *= input.opacity * saturate(0.5 - distance) *
             rounded_mask_coverage(input.position.xy, sprite.content_mask.rounded);
  return color;
}

struct PathTileFragmentInput {
  float4 position [[position]];
  uint tile_id [[flat]];
  float4 curve_box [[flat]];
  PreparedBackground background;
  uint rounded [[flat]];
};

vertex PathTileFragmentInput path_tile_vertex(
    uint vertex_id [[vertex_id]], uint tile_id [[instance_id]],
    constant PathTile *tiles [[buffer(PathTileInputIndex_Tiles)]],
    constant PathPaint *paints [[buffer(PathTileInputIndex_Paints)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]]) {
  PathTile tile = tiles[tile_id];
  PathPaint paint = paints[tile.paint];

  float2 mask_top_left = float2(paint.content_mask.bounds.origin.x,
                                paint.content_mask.bounds.origin.y);
  float2 mask_bottom_right =
      mask_top_left + float2(paint.content_mask.bounds.size.width,
                             paint.content_mask.bounds.size.height);
  float2 top_left = clamp(float2(tile.top_left.x, tile.top_left.y),
                          mask_top_left, mask_bottom_right);
  float2 bottom_right = clamp(float2(tile.bottom_right.x, tile.bottom_right.y),
                              top_left, mask_bottom_right);
  float2 position =
      float2((vertex_id & 1u) != 0u ? bottom_right.x : top_left.x,
             (vertex_id & 2u) != 0u ? bottom_right.y : top_left.y);

  return PathTileFragmentInput{
      to_device_position_impl(position, viewport_size),
      tile_id,
      float4(tile.curves_top_left.x, tile.curves_top_left.y,
             tile.curves_bottom_right.x, tile.curves_bottom_right.y),
      prepare_background(paint.color),
      has_rounded_mask(paint.content_mask) ? 1u : 0u};
}

// Solves a*t^2 + b*t + c = 0 for the root in [0, 1] on a monotone piece.
float monotone_quadratic_root(float a, float b, float c, float direction) {
  float discriminant = max(b * b - 4.0 * a * c, 0.0);
  float denominator = b + direction * sqrt(discriminant);
  if (denominator == 0.0) {
    return 0.0;
  }
  return saturate(-2.0 * c / denominator);
}

// Coverage estimate adapted from https://github.com/EricLengyel/Slug/blob/be3c13eb7d63f9e8aa5c583e42d92c374cb91d98/SlugPixelShader.hlsl
// Copyright (c) 2017 Eric Lengyel.
// Licensed under the MIT license.

// Accumulates the curve's winding along the pixel's leftward and upward rays.
// Like Slug, a crossing within half a pixel of the center counts fractionally.
void add_curve_winding(PathCurve curve, float2 center, thread float2 &winding,
                       thread float2 &weight) {
  float2 p0 = float2(curve.p0.x, curve.p0.y);
  float2 p1 = float2(curve.p1.x, curve.p1.y);
  float2 a = float2(curve.a.x, curve.a.y);
  float2 b = float2(curve.b.x, curve.b.y);
  float min_x = min(p0.x, p1.x);
  float max_x = max(p0.x, p1.x);

  if (p0.y <= center.y && center.y < p1.y) {
    if (max_x <= center.x - 0.5) {
      winding.x += curve.sign;
    } else {
      float t = monotone_quadratic_root(a.y, b.y, p0.y - center.y, 1.0);
      float x = (a.x * t + b.x) * t + p0.x;
      float distance = center.x - x;
      winding.x += curve.sign * saturate(distance + 0.5);
      weight.x = max(weight.x, saturate(1.0 - 2.0 * abs(distance)));
    }
  }

  if (min_x <= center.x && center.x < max_x) {
    float direction = p1.x > p0.x ? 1.0 : -1.0;
    if (p1.y <= center.y - 0.5) {
      winding.y -= curve.sign * direction;
    } else if (p0.y < center.y + 0.5) {
      float t = monotone_quadratic_root(a.x, b.x, p0.x - center.x, direction);
      float y = (a.y * t + b.y) * t + p0.y;
      float distance = center.y - y;
      winding.y -= curve.sign * direction * saturate(distance + 0.5);
      weight.y = max(weight.y, saturate(1.0 - 2.0 * abs(distance)));
    }
  }
}

// The backdrop gives the unlisted curves' winding along a leftward ray from
// the tile's reference corner. We need it at the current pixel, along both
// leftward and upward rays. Because paths are closed, chains of unlisted
// curves can only begin or end where a listed curve does. So, we can compute
// the winding at the current pixel by correcting the backdrop for each listed
// endpoint between its ray and the pixel.
void correct_for_endpoint(float2 endpoint, float step, float2 reference,
                          float2 center, thread float2 &winding) {
  if (endpoint.x <= reference.x) {
    winding.x -= step * (float(endpoint.y <= center.y) -
                         float(endpoint.y <= reference.y));
  }
  if (endpoint.y <= reference.y && endpoint.x <= center.x) {
    winding.y += step;
  }
}

fragment float4 path_tile_fragment(
    PathTileFragmentInput input [[stage_in]],
    constant PathTile *tiles [[buffer(PathTileInputIndex_Tiles)]],
    constant PathCurve *curves [[buffer(PathTileInputIndex_Curves)]],
    constant PathPaint *paints [[buffer(PathTileInputIndex_Paints)]]) {
  float2 center = floor(input.position.xy) + 0.5;
  if (input.background.tag == 0u && input.rounded == 0u &&
      (any(center < input.curve_box.xy) || any(center > input.curve_box.zw))) {
    return input.background.color0;
  }
  PathTile tile = tiles[input.tile_id];
  PathPaint paint = paints[tile.paint];
  float mask_coverage =
      rounded_mask_coverage(input.position.xy, paint.content_mask.rounded);
  if (input.background.tag == 0u &&
      (any(center < input.curve_box.xy) || any(center > input.curve_box.zw))) {
    return input.background.color0 * float4(1.0, 1.0, 1.0, mask_coverage);
  }
  float2 reference = float2(tile.reference.x, tile.reference.y);

  float2 winding = float2(float(tile.backdrop));
  float2 weight = float2(0.0);
  for (uint i = 0u; i < tile.curve_count; i++) {
    PathCurve curve = curves[tile.curve_start + i];
    float2 p0 = float2(curve.p0.x, curve.p0.y);
    float2 p1 = float2(curve.p1.x, curve.p1.y);
    if (min(p0.x, p1.x) >= center.x + 0.5) {
      break;
    }
    if (p0.y < center.y + 0.5) {
      add_curve_winding(curve, center, winding, weight);
      correct_for_endpoint(p0, curve.sign, reference, center, winding);
      correct_for_endpoint(p1, -curve.sign, reference, center, winding);
    }
  }

  // Weight the winding computed by each ray by its nearest crossing's distance
  // from the pixel center, using Slug's technique.
  float coverage =
      max(abs(dot(winding, weight)) / max(weight.x + weight.y, 1.0 / 65536.0),
          min(abs(winding.x), abs(winding.y)));
  coverage = paint.even_odd != 0u
                 ? 1.0 - abs(1.0 - fract(coverage * 0.5) * 2.0)
                 : saturate(coverage);

  if (coverage == 0.0) {
    return float4(0.0);
  }

  float4 color = background_color(input.background, paint.color,
                                  paint.bounds, input.position.xy);
  return float4(color.rgb, color.a * coverage * mask_coverage);
}

struct SurfaceVertexOutput {
  float4 position [[position]];
  float2 texture_position;
  float4 mask_bounds [[flat]];
  float4 mask_radii [[flat]];
};

struct SurfaceFragmentInput {
  float4 position [[position]];
  float2 texture_position;
  float4 mask_bounds [[flat]];
  float4 mask_radii [[flat]];
};

// QUILL: a surface's rounded mask, carried to the fragment as flat values.
float surface_mask_coverage(SurfaceFragmentInput input) {
  RoundedMask_ScaledPixels mask;
  mask.bounds.origin.x = input.mask_bounds.x;
  mask.bounds.origin.y = input.mask_bounds.y;
  mask.bounds.size.width = input.mask_bounds.z;
  mask.bounds.size.height = input.mask_bounds.w;
  mask.radii.top_left = input.mask_radii.x;
  mask.radii.top_right = input.mask_radii.y;
  mask.radii.bottom_right = input.mask_radii.z;
  mask.radii.bottom_left = input.mask_radii.w;
  return rounded_mask_coverage(input.position.xy, mask);
}

vertex SurfaceVertexOutput surface_vertex(
    uint vertex_id [[vertex_id]], uint surface_id [[instance_id]],
    constant SurfaceBounds *surfaces [[buffer(SurfaceInputIndex_Surfaces)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]],
    constant Size_DevicePixels *texture_size
    [[buffer(SurfaceInputIndex_TextureSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  SurfaceBounds surface = surfaces[surface_id];
  ClippedVertex clipped =
      clip_to_mask(unit_vertex, surface.bounds, surface.content_mask.bounds);
  float4 device_position =
      to_device_position_impl(clipped.position, viewport_size);
  // We are going to copy the whole texture, so the texture position corresponds
  // to the vertex's position within the unclipped surface bounds.
  float2 texture_position = clipped.unit_vertex;
  RoundedMask_ScaledPixels rounded = surface.content_mask.rounded;
  return SurfaceVertexOutput{
      device_position,
      texture_position,
      float4(rounded.bounds.origin.x, rounded.bounds.origin.y,
             rounded.bounds.size.width, rounded.bounds.size.height),
      float4(rounded.radii.top_left, rounded.radii.top_right,
             rounded.radii.bottom_right, rounded.radii.bottom_left)};
}

fragment float4 surface_fragment(SurfaceFragmentInput input [[stage_in]],
                                 texture2d<float> y_texture
                                 [[texture(SurfaceInputIndex_YTexture)]],
                                 texture2d<float> cb_cr_texture
                                 [[texture(SurfaceInputIndex_CbCrTexture)]]) {
  constexpr sampler texture_sampler(mag_filter::linear, min_filter::linear);
  const float4x4 ycbcrToRGBTransform =
      float4x4(float4(+1.0000f, +1.0000f, +1.0000f, +0.0000f),
               float4(+0.0000f, -0.3441f, +1.7720f, +0.0000f),
               float4(+1.4020f, -0.7141f, +0.0000f, +0.0000f),
               float4(-0.7010f, +0.5291f, -0.8860f, +1.0000f));
  float4 ycbcr = float4(
      y_texture.sample(texture_sampler, input.texture_position).r,
      cb_cr_texture.sample(texture_sampler, input.texture_position).rg, 1.0);

  float4 rgba = ycbcrToRGBTransform * ycbcr;
  rgba.a *= surface_mask_coverage(input);
  return rgba;
}

struct NativeViewVertexOutput {
  float4 position [[position]];
};

vertex NativeViewVertexOutput native_view_vertex(
    uint vertex_id [[vertex_id]], uint native_view_id [[instance_id]],
    constant SurfaceBounds *native_views [[buffer(SurfaceInputIndex_Surfaces)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(GlobalInputIndex_ViewportSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  SurfaceBounds native_view = native_views[native_view_id];
  ClippedVertex clipped = clip_to_mask(unit_vertex, native_view.bounds,
                                       native_view.content_mask.bounds);
  return NativeViewVertexOutput{
      to_device_position_impl(clipped.position, viewport_size)};
}

// The pipeline does not blend, so this erases what was drawn beneath the view.
fragment float4 native_view_fragment(NativeViewVertexOutput input [[stage_in]]) {
  return float4(0.);
}

// QUILL: HDR still images (`kCVPixelFormatType_64RGBAHalf`, linear extended
// sRGB, premultiplied alpha). `headroom` is the display's EDR headroom (1 on
// an SDR target); `extended_output` is set when the drawable is a float
// layer in extended sRGB.
struct HdrSurfaceParams {
  float headroom;
  uint extended_output;
};

// Identity up to 80% of the headroom, then a smooth roll-off that reaches
// the headroom only at infinity (slope 1 at the knee). Twin of
// `hdr_tone_map` in Quill's `src/ui/hdr_image/tone_map.rs`.
float hdr_tone_map(float x, float headroom) {
  float knee = 0.8 * headroom;
  if (x <= knee) {
    return x;
  }
  float span = headroom - knee;
  float over = x - knee;
  return knee + span * (over / (over + span));
}

// The sRGB transfer function, mirrored for negative values (extended sRGB
// keeps out-of-gamut colors as negatives).
float srgb_encode(float value) {
  float magnitude = fabs(value);
  float encoded = magnitude <= 0.0031308 ? magnitude * 12.92
                                         : 1.055 * pow(magnitude, 1.0 / 2.4) - 0.055;
  return copysign(encoded, value);
}

fragment float4 hdr_surface_fragment(
    SurfaceFragmentInput input [[stage_in]],
    texture2d<half> image [[texture(SurfaceInputIndex_YTexture)]],
    constant HdrSurfaceParams *params [[buffer(0)]]) {
  constexpr sampler texture_sampler(mag_filter::linear, min_filter::linear);
  float4 color = float4(image.sample(texture_sampler, input.texture_position));
  float alpha = color.a;
  float3 rgb = alpha > 0.0 ? color.rgb / alpha : float3(0.0);
  bool extended = params->extended_output != 0;
  float headroom = max(params->headroom, 1.0);
  float peak = max(max(rgb.r, rgb.g), rgb.b);
  // An 8-bit drawable is a passing frame (EDR switching on or off): clip
  // rather than dim SDR white with the knee.
  if (extended && peak > 0.0) {
    rgb *= hdr_tone_map(peak, headroom) / peak;
  }
  if (!extended) {
    rgb = clamp(rgb, 0.0, 1.0);
  }
  return float4(srgb_encode(rgb.r), srgb_encode(rgb.g), srgb_encode(rgb.b),
                alpha * surface_mask_coverage(input));
}

float4 hsla_to_rgba(Hsla hsla) {
  float h = hsla.h * 6.0; // Now, it's an angle but scaled in [0, 6) range
  float s = hsla.s;
  float l = hsla.l;
  float a = hsla.a;

  float c = (1.0 - fabs(2.0 * l - 1.0)) * s;
  float x = c * (1.0 - fabs(fmod(h, 2.0) - 1.0));
  float m = l - c / 2.0;

  float r = 0.0;
  float g = 0.0;
  float b = 0.0;

  if (h >= 0.0 && h < 1.0) {
    r = c;
    g = x;
    b = 0.0;
  } else if (h >= 1.0 && h < 2.0) {
    r = x;
    g = c;
    b = 0.0;
  } else if (h >= 2.0 && h < 3.0) {
    r = 0.0;
    g = c;
    b = x;
  } else if (h >= 3.0 && h < 4.0) {
    r = 0.0;
    g = x;
    b = c;
  } else if (h >= 4.0 && h < 5.0) {
    r = x;
    g = 0.0;
    b = c;
  } else {
    r = c;
    g = 0.0;
    b = x;
  }

  float4 rgba;
  rgba.x = (r + m);
  rgba.y = (g + m);
  rgba.z = (b + m);
  rgba.w = a;
  return rgba;
}

float3 srgb_to_linear(float3 color) {
  return pow(color, float3(2.2));
}

float3 linear_to_srgb(float3 color) {
  return pow(color, float3(1.0 / 2.2));
}

// Converts a sRGB color to the Oklab color space.
// Reference: https://bottosson.github.io/posts/oklab/#converting-from-linear-srgb-to-oklab
float4 srgb_to_oklab(float4 color) {
  // Convert non-linear sRGB to linear sRGB
  color = float4(srgb_to_linear(color.rgb), color.a);

  float l = 0.4122214708 * color.r + 0.5363325363 * color.g + 0.0514459929 * color.b;
  float m = 0.2119034982 * color.r + 0.6806995451 * color.g + 0.1073969566 * color.b;
  float s = 0.0883024619 * color.r + 0.2817188376 * color.g + 0.6299787005 * color.b;

  float l_ = pow(l, 1.0/3.0);
  float m_ = pow(m, 1.0/3.0);
  float s_ = pow(s, 1.0/3.0);

  return float4(
   	0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
   	1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
   	0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
   	color.a
  );
}

// Converts an Oklab color to the sRGB color space.
float4 oklab_to_srgb(float4 color) {
  float l_ = color.r + 0.3963377774 * color.g + 0.2158037573 * color.b;
  float m_ = color.r - 0.1055613458 * color.g - 0.0638541728 * color.b;
  float s_ = color.r - 0.0894841775 * color.g - 1.2914855480 * color.b;

  float l = l_ * l_ * l_;
  float m = m_ * m_ * m_;
  float s = s_ * s_ * s_;

  float3 linear_rgb = float3(
   	4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
   	-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
   	-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
  );

  // Convert linear sRGB to non-linear sRGB
  return float4(linear_to_srgb(linear_rgb), color.a);
}

float4 to_device_position_impl(float2 position,
                               constant Size_DevicePixels *input_viewport_size) {
  float2 viewport_size = float2((float)input_viewport_size->width,
                                (float)input_viewport_size->height);
  float2 device_position =
      position / viewport_size * float2(2., -2.) + float2(-1., 1.);
  return float4(device_position, 0., 1.);
}

float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *input_viewport_size) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  return to_device_position_impl(position, input_viewport_size);
}

float4 to_device_position_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          TransformationMatrix transformation,
                          constant Size_DevicePixels *input_viewport_size) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);

  // Apply the transformation matrix to the position via matrix multiplication.
  float2 transformed_position = float2(0, 0);
  transformed_position[0] = position[0] * transformation.rotation_scale[0][0] + position[1] * transformation.rotation_scale[0][1];
  transformed_position[1] = position[0] * transformation.rotation_scale[1][0] + position[1] * transformation.rotation_scale[1][1];

  // Add in the translation component of the transformation matrix.
  transformed_position[0] += transformation.translation[0];
  transformed_position[1] += transformation.translation[1];

  float2 viewport_size = float2((float)input_viewport_size->width,
                                (float)input_viewport_size->height);
  float2 device_position =
      transformed_position / viewport_size * float2(2., -2.) + float2(-1., 1.);
  return float4(device_position, 0., 1.);
}


float2 to_tile_position(float2 unit_vertex, AtlasTile tile,
                        constant Size_DevicePixels *atlas_size) {
  float2 tile_origin = float2(tile.bounds.origin.x, tile.bounds.origin.y);
  float2 tile_size = float2(tile.bounds.size.width, tile.bounds.size.height);
  return (tile_origin + unit_vertex * tile_size) /
         float2((float)atlas_size->width, (float)atlas_size->height);
}

// Selects corner radius based on quadrant.
float pick_corner_radius(float2 center_to_point, Corners_ScaledPixels corner_radii) {
  if (center_to_point.x < 0.) {
    if (center_to_point.y < 0.) {
      return corner_radii.top_left;
    } else {
      return corner_radii.bottom_left;
    }
  } else {
    if (center_to_point.y < 0.) {
      return corner_radii.top_right;
    } else {
      return corner_radii.bottom_right;
    }
  }
}

// Signed distance of the point to the quad's border - positive outside the
// border, and negative inside.
float quad_sdf(float2 point, Bounds_ScaledPixels bounds,
               Corners_ScaledPixels corner_radii) {
    float2 half_size = float2(bounds.size.width, bounds.size.height) / 2.0;
    float2 center = float2(bounds.origin.x, bounds.origin.y) + half_size;
    float2 center_to_point = point - center;
    float corner_radius = pick_corner_radius(center_to_point, corner_radii);
    float2 corner_to_point = fabs(center_to_point) - half_size;
    float2 corner_center_to_point = corner_to_point + corner_radius;
    return quad_sdf_impl(corner_center_to_point, corner_radius);
}

// QUILL: the coverage of a rounded content mask at a device position, 1 when
// the mask has no rounded shape (docs/decisions/rounded-content-masks.md).
float rounded_mask_coverage(float2 position, RoundedMask_ScaledPixels mask) {
  if (!has_rounded_corners(mask.radii)) {
    return 1.0;
  }
  return saturate(0.5 - quad_sdf(position, mask.bounds, mask.radii));
}

bool has_rounded_mask(ContentMask_ScaledPixels mask) {
  return has_rounded_corners(mask.rounded.radii);
}

// Implementation of quad signed distance field
float quad_sdf_impl(float2 corner_center_to_point, float corner_radius) {
    if (corner_radius == 0.0) {
        // Fast path for unrounded corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius
        // It is negative inside this quad, and positive outside
        float signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside
            length(max(float2(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));

        return signed_distance_to_inset_quad - corner_radius;
    }
}

ClippedVertex clip_to_mask(float2 unit_vertex, Bounds_ScaledPixels bounds,
                           Bounds_ScaledPixels mask) {
  float2 bounds_origin = float2(bounds.origin.x, bounds.origin.y);
  float2 bounds_size = float2(bounds.size.width, bounds.size.height);
  float2 mask_origin = float2(mask.origin.x, mask.origin.y);
  float2 mask_size = float2(mask.size.width, mask.size.height);
  float2 origin = max(bounds_origin, mask_origin);
  float2 corner = min(bounds_origin + bounds_size, mask_origin + mask_size);
  float2 size = max(corner - origin, float2(0.));

  ClippedVertex result;
  result.position = origin + unit_vertex * size;
  result.unit_vertex = (result.position - bounds_origin) / bounds_size;
  return result;
}

float4 distance_from_clip_rect_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds, TransformationMatrix transformation) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  float2 transformed_position = float2(0, 0);
  transformed_position[0] = position[0] * transformation.rotation_scale[0][0] + position[1] * transformation.rotation_scale[0][1];
  transformed_position[1] = position[0] * transformation.rotation_scale[1][0] + position[1] * transformation.rotation_scale[1][1];
  transformed_position[0] += transformation.translation[0];
  transformed_position[1] += transformation.translation[1];

  return float4(transformed_position.x - clip_bounds.origin.x,
                clip_bounds.origin.x + clip_bounds.size.width - transformed_position.x,
                transformed_position.y - clip_bounds.origin.y,
                clip_bounds.origin.y + clip_bounds.size.height - transformed_position.y);
}

float4 over(float4 below, float4 above) {
  float alpha = above.a + below.a * (1.0 - above.a);
  float3 premultiplied =
      above.rgb * above.a + below.rgb * below.a * (1.0 - above.a);
  return float4(premultiplied / (alpha == 0.0 ? 1.0 : alpha), alpha);
}

PreparedBackground prepare_background(Background background) {
  PreparedBackground prepared;
  prepared.tag = background.tag;
  prepared.color1 = float4(0.0);

  if (background.tag == 1u) {
    prepared.color0 = hsla_to_rgba(background.colors[0].color);
    prepared.color1 = hsla_to_rgba(background.colors[1].color);
    // Interpolating in Oklab means converting the endpoints once rather
    // than at every stop evaluation.
    if (background.color_space == 1u) {
      prepared.color0 = srgb_to_oklab(prepared.color0);
      prepared.color1 = srgb_to_oklab(prepared.color1);
    }
  } else {
    prepared.color0 = hsla_to_rgba(background.solid);
  }

  return prepared;
}

// Ordered (Bayer 4x4) dither threshold, remapped to [-1, +1].
float dither_offset(float2 position) {
  uint2 cell = uint2(position) & uint2(3u);
  uint z = cell.x ^ cell.y;
  uint threshold = ((z & 1u) << 3) | ((cell.y & 1u) << 2) | (z & 2u) |
                   ((cell.y & 2u) >> 1);
  return (float(threshold) + 0.5) * (1.0 / 8.0) - 1.0;
}

float4 background_color(PreparedBackground prepared, Background background,
                        Bounds_ScaledPixels bounds, float2 position) {
  float4 color = prepared.color0;
  if (prepared.tag == 0u) {
    return color;
  }

  float2 origin = float2(bounds.origin.x, bounds.origin.y);
  float2 size = float2(bounds.size.width, bounds.size.height);

  switch (prepared.tag) {
    case 1: {
      // -90 degrees to match the CSS gradient angle.
      float radians = (fmod(background.gradient_angle_or_pattern_height, 360.0) - 90.0) *
                      (M_PI_F / 180.0);
      float2 direction = float2(cos(radians), sin(radians));

      // Expand the short side to be the same as the long side
      if (size.x > size.y) {
        direction.y *= size.y / size.x;
      } else {
        direction.x *= size.x / size.y;
      }

      float extent = abs(direction.x) > abs(direction.y) ? size.x : size.y;
      float direction_length = length(direction);
      float span = background.colors[1].percentage - background.colors[0].percentage;
      // The half-extent term is exactly half of `extent`, so the
      // position-independent part reduces to this.
      float offset = 0.5 - background.colors[0].percentage;

      // t = dot(position - center, gradient.xy) + gradient.z
      float3 gradient = float3(0.0);
      if (direction_length != 0.0 && extent != 0.0) {
        if (span == 0.0) {
          const float STEP = 1e30;
          gradient = float3(direction / (direction_length * extent) * STEP, offset * STEP);
        } else {
          gradient = float3(direction / (direction_length * extent * span), offset / span);
        }
      }
      float2 center = origin + size * 0.5;
      float t = saturate(dot(position - center, gradient.xy) + gradient.z);
      color = mix(prepared.color0, prepared.color1, t);
      if (background.color_space == 1u) {
        color = oklab_to_srgb(color);
      }

      // Dither to reduce banding in gradients (especially dark/alpha).
      float dither = dither_offset(position);
      color.rgb += dither * 1.0 / 255.0;
      color.a += dither * 1.5 / 255.0;
      break;
    }
    case 2: {
      float pattern_height_encoded = background.gradient_angle_or_pattern_height;
      float pattern_width = (pattern_height_encoded / 65535.0) / 255.0;
      float pattern_interval = fmod(pattern_height_encoded, 65535.0) / 255.0;
      float pattern_height = pattern_width + pattern_interval;
      // Only the rotated x coordinate is used, and multiplying by the
      // rotation reduces it to dot(v, float2(cos a, sin a)).
      float stripe_angle = M_PI_F / 4.0;
      float2 stripe_direction = float2(cos(stripe_angle), sin(stripe_angle));
      float pattern_period = pattern_height * sin(stripe_angle);
      float threshold = pattern_period * (pattern_width / pattern_height) / 2.0;
      float pattern = fmod(dot(position - origin, stripe_direction), pattern_period);
      float distance = min(pattern, pattern_period - pattern) - threshold;
      color.a *= saturate(0.5 - distance);
      break;
    }
    case 3: {
      // checkerboard
      float cell_size = background.gradient_angle_or_pattern_height;
      float2 relative_position = position - origin;
      float x_index = floor(relative_position.x / cell_size);
      float y_index = floor(relative_position.y / cell_size);
      color.a *= saturate(fmod(x_index + y_index, 2.0));
      break;
    }
  }

  return color;
}

bool has_rounded_corners(Corners_ScaledPixels corner_radii) {
  return corner_radii.top_left != 0.0 || corner_radii.top_right != 0.0 ||
         corner_radii.bottom_right != 0.0 || corner_radii.bottom_left != 0.0;
}

bool has_border(Edges_ScaledPixels border_widths) {
  return border_widths.top != 0.0 || border_widths.right != 0.0 ||
         border_widths.bottom != 0.0 || border_widths.left != 0.0;
}

bool is_shaped(Quad quad) {
  return has_rounded_corners(quad.corner_radii) || has_border(quad.border_widths);
}

// QUILL: backdrop glass (`BackdropLens`). The renderer copies each lens's
// footprint out of the target into a slot of a mip-chained atlas, blurs the
// chain with dual-Kawase downsamples, and draws the lens from it. The optics
// are gpui-liquid-glass's CPU reference (`optics.rs`, after
// bilalgursen/liquid-glass, MIT), in design units of a lens 120 units tall.
// See third_party/gpui-pre-apple/QUILL-CHANGES.md.

constant int LENS_LEVELS = 5;

struct LensVertexOutput {
  uint lens_id [[flat]];
  float4 position [[position]];
  float clip_distance [[clip_distance]][4];
};

struct LensFragmentInput {
  uint lens_id [[flat]];
  float4 position [[position]];
};

// The vertex's distance inside each edge of the clip rect, for an
// untransformed quad (0.3.14 dropped this helper from upstream's shaders).
float4 distance_from_clip_rect(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  return float4(position.x - clip_bounds.origin.x,
                clip_bounds.origin.x + clip_bounds.size.width - position.x,
                position.y - clip_bounds.origin.y,
                clip_bounds.origin.y + clip_bounds.size.height - position.y);
}

vertex LensVertexOutput lens_vertex(uint vertex_id [[vertex_id]],
                                    uint lens_id [[instance_id]],
                                    constant BackdropLens *lenses
                                    [[buffer(LensInputIndex_Lenses)]],
                                    constant Size_DevicePixels *viewport_size
                                    [[buffer(GlobalInputIndex_ViewportSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  BackdropLens lens = lenses[lens_id];
  float4 device_position =
      to_device_position(unit_vertex, lens.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, lens.bounds,
                                                 lens.content_mask.bounds);
  return LensVertexOutput{
      lens_id,
      device_position,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

// `smoothstep` as optics.rs has it: either edge may be the larger one.
float lens_step(float a, float b, float v) {
  float t = clamp((v - a) / (b - a), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}

float lens_dim(float k) { return pow(clamp(k, 0.0, 1.0), 1.0 / 2.2); }

float lens_round_rect(float2 p, float2 b, float r) {
  float2 q = abs(p) - b + r;
  return length(max(q, float2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

float3 lens_saturate(float3 c, float s) {
  if (abs(s - 1.0) < 1e-4) {
    return c;
  }
  float l = dot(c, float3(0.2126, 0.7152, 0.0722));
  return max(mix(float3(l), c, s), float3(0.0));
}

// Everything per lens the pixels share, from `BackdropLens` (optics.rs
// `Prepared`). Lengths are units.
struct LensShape {
  float2 center;   // device pixels
  float unit_dev;  // device pixels per unit
  float device_px; // a device pixel, in units
  float css_px;    // a logical pixel, in units
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
  LensShape s;
  float scale = max(lens.scale_factor, 1e-3);
  float unit = max(lens.unit, 1e-3);
  s.unit_dev = unit * scale;
  s.device_px = 1.0 / s.unit_dev;
  s.css_px = 1.0 / unit;
  float2 body_origin = float2(lens.body.origin.x, lens.body.origin.y);
  float2 body_size = float2(lens.body.size.width, lens.body.size.height);
  s.center = body_origin + body_size * 0.5;
  float2 half_logical = body_size * 0.5 / scale;
  s.half_size = max(half_logical / unit, float2(0.5));
  float radius = min(lens.corner_radius / scale,
                     min(half_logical.x - 0.5, half_logical.y - 0.5));
  s.radius = max(radius, 0.0) / unit;
  s.band = lens.material == 0 ? lens.band : 0.0;
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
  return mix(a, b, h) - k * h * (1.0 - h);
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
float3 lens_back(LensShape s, LensSlot slot, texture2d<float> atlas,
                 float2 atlas_size, float2 p, float lod) {
  constexpr sampler back_sampler(mag_filter::linear, min_filter::linear,
                                 mip_filter::linear, address::clamp_to_edge);
  // A re-rendered slot holds `density` texels per device pixel: the same
  // blur is that many levels further down.
  float density = max(slot.density, 1.0);
  float mip = clamp(lod + log2(density), 0.0, float(LENS_LEVELS - 1));
  // Its levels below the sharpest are blurred, not just smaller: a sharp
  // read stays on whole levels (supersampled level 0 for a magnifier).
  if (density > 1.0) {
    mip = floor(mip);
  }
  float2 window = s.center + p * s.unit_dev;
  float2 copy_origin = float2(slot.copy_x, slot.copy_y);
  float2 copy_size = float2(slot.copy_w, slot.copy_h);
  float inset = 0.5 * exp2(ceil(mip)) / density;
  float2 lo = float2(inset);
  float2 hi = max(copy_size - inset, lo);
  float2 local = clamp(window - copy_origin, lo, hi);
  float2 uv = (float2(slot.slot_x, slot.slot_y) + local * density) / atlas_size;
  return atlas.sample(back_sampler, uv, level(mip)).rgb;
}

// A gaussian of `sigma` device pixels, from the mip chain: level k holds
// about sigma 0.87 * 2^k, eight taps around the point widen it.
float3 lens_frosted(LensShape s, LensSlot slot, texture2d<float> atlas,
                    float2 atlas_size, float2 p, float sigma) {
  float lod = log2(max(sigma, 0.5) / 0.87);
  float mip = clamp(lod, 0.0, float(LENS_LEVELS - 1));
  float reach = 0.75 * exp2(mip) * max(exp2(lod - mip), 1.0) / s.unit_dev;
  float3 sum = float3(0.0);
  for (int i = 0; i < 8; i++) {
    float angle = float(i) * (M_PI_F / 4.0) + 0.3;
    float r = (i % 2 == 0) ? reach : reach * 0.55;
    sum += lens_back(s, slot, atlas, atlas_size,
                     p + float2(cos(angle), sin(angle)) * r, mip);
  }
  return sum / 8.0;
}

float3 lens_bent(LensShape s, LensSlot slot, texture2d<float> atlas,
                 float2 atlas_size, float2 p, float2 bend, float split,
                 float lod) {
  float r = lens_back(s, slot, atlas, atlas_size, p + bend * (1.0 + split), lod).r;
  float g = lens_back(s, slot, atlas, atlas_size, p + bend, lod).g;
  float b = lens_back(s, slot, atlas, atlas_size, p + bend * (1.0 - split), lod).b;
  return float3(r, g, b);
}

float2 lens_radial_bend(LensShape s, BackdropLens lens, float2 q,
                        float amount) {
  float d = lens_sdf(s, q);
  float len = max(length(q), 1e-4);
  float ratio = clamp(1.0 - abs(d) / (min(s.half_size.x, s.half_size.y) *
                                      s.edge_band / 0.35),
                      0.0, 1.0);
  float k = pow(ratio, lens.tune_bend_curve) *
            lens_step(1.5 * s.css_px, -0.5 * s.css_px, d);
  return -q / len * k * amount;
}

fragment float4 lens_fragment(LensFragmentInput input [[stage_in]],
                              constant BackdropLens *lenses
                              [[buffer(LensInputIndex_Lenses)]],
                              constant LensSlot *slots
                              [[buffer(LensInputIndex_Slots)]],
                              constant Size_DevicePixels *atlas_size_px
                              [[buffer(LensInputIndex_AtlasSize)]],
                              texture2d<float> atlas
                              [[texture(LensInputIndex_Atlas)]]) {
  BackdropLens lens = lenses[input.lens_id];
  LensSlot slot = slots[input.lens_id];
  float mask_coverage =
      rounded_mask_coverage(input.position.xy, lens.content_mask.rounded);
  float2 atlas_size =
      float2((float)atlas_size_px->width, (float)atlas_size_px->height);
  LensShape s = lens_shape(lens);
  float strength = lens.strength;
  bool clear = lens.material == 0;

  float2 local = (input.position.xy - s.center) / s.unit_dev;
  float dist = lens_sdf(s, local);

  // Drop shadow: tight + ambient, faded before the footprint's edge.
  float v_fade = lens_step(-0.1, 0.9, local.y / s.half_size.y) * 0.92 + 0.08;
  float outside = lens_step(-4.0, 14.0, dist);
  float aa = max(s.device_px * 1.2, 1e-4);
  float mask = 1.0 - lens_step(-aa, aa, dist);
  // QUILL: deeper than 4 units inside the rim `outside` is 0, and so is the
  // shadow: its two distance fields are skipped there (the same pixels).
  float sh = 0.0;
  if (outside > 0.0) {
    float soft = lens.shadow_softness;
    float off = lens.shadow_offset;
    float near =
        1.0 - lens_step(-6.0, soft, lens_sdf(s, local - float2(0.0, off)));
    float far = 1.0 - lens_step(-10.0, soft * 1.5,
                                lens_sdf(s, local - float2(0.0, off * 1.6)));
    float2 in_footprint =
        input.position.xy - float2(lens.bounds.origin.x, lens.bounds.origin.y);
    float border = min(min(in_footprint.x, in_footprint.y),
                       min(lens.bounds.size.width - in_footprint.x,
                           lens.bounds.size.height - in_footprint.y));
    sh = clamp((near + far) * lens.shadow * strength * v_fade * outside, 0.0,
               0.85) *
         lens_step(0.0, 14.0, border);
  }
  float shadow_alpha = 1.0 - lens_dim(1.0 - sh);
  if (mask <= 0.0) {
    if (shadow_alpha <= 0.0) {
      discard_fragment();
    }
    return float4(0.0, 0.0, 0.0, shadow_alpha * lens.opacity * mask_coverage);
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
  // QUILL: a frost mixed in whole (every slab's) replaces the sharp bent
  // backdrop, so its three taps are skipped (the same pixels, to the
  // rounding of `mix(bent, frost, 1.0)`).
  bool frosted_through = frost_sigma > 0.0 && lens.blur_mix == 1.0;
  float3 glass = float3(0.0);
  if (!clear) {
    glass = mix(surface.rgb, color.rgb, 1.0 - edge * lens.edge_clear);
  } else if (lens.bend == 1) {
    float amount = lens.radial_amount / max(lens.unit, 1e-3);
    float2 bend = lens_radial_bend(s, lens, local, amount);
    if (!frosted_through) {
      glass = lens_bent(s, slot, atlas, atlas_size, local, bend, 0.0,
                        max(s.blur_shift, 0.0));
    }
    if (frost_sigma > 0.0) {
      float ratio = clamp(1.0 - abs(dist) / (min(s.half_size.x, s.half_size.y) *
                                             s.edge_band / 0.35),
                          0.0, 1.0);
      float sigma = frost_sigma * mix(1.0, s.frost_edge, pow(ratio, 1.3));
      float3 frost = lens_frosted(s, slot, atlas, atlas_size, local + bend,
                                  max(sigma, 0.5));
      glass = frosted_through ? frost : mix(glass, frost, lens.blur_mix);
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
    // magnified middle reads its backdrop that much finer (a re-rendered
    // backdrop has the detail for it).
    float lod = max(mix(lens.blur_center, lens.blur_edge, pow(bend_edge, 1.3)) +
                        s.blur_shift,
                    0.0) -
                log2(mag);
    if (!frosted_through) {
      glass = lens_bent(s, slot, atlas, atlas_size, at, bend, split, lod);
    }
    if (frost_sigma > 0.0) {
      float sigma =
          frost_sigma * mix(1.0, s.frost_edge, pow(bend_edge, 1.3));
      float3 frost = lens_frosted(s, slot, atlas, atlas_size, at + bend,
                                  max(sigma, 0.5));
      glass = frosted_through ? frost : mix(glass, frost, lens.blur_mix);
    }
    glass = lens_saturate(glass, lens.tune_saturation);
  }

  // Top reflection.
  float ny = local.y / s.half_size.y;
  float nx = local.x / s.half_size.x;
  float top_refl = 0.3 * strength * lens.tune_reflection;
  float top_base = 0.02 * strength * lens.tune_reflection;
  float top = (1.0 - lens_step(-0.05, 0.75, ny)) * (1.0 - nx * 0.12) *
                  top_refl * 0.6 +
              top_base;
  glass *= float3(lens_dim(1.0 - top), lens_dim(1.0 - top),
                  lens_dim(1.0 - 0.72 * top));

  // Frosted resting state. QUILL (glass effects): `edge_clear` thins the
  // frost across the rim band (`LensTune::rim_frost`), so the rim shows
  // the bent backdrop while the middle keeps its tint.
  if (clear) {
    glass = mix(glass, color.rgb, lens.frost * (1.0 - edge * lens.edge_clear));
  }

  // Inner shading: just inside the rim, a wide band on the left, a deep ring.
  float inner = 0.1 * strength * lens.tune_inner;
  float up = clamp(-dir.y, 0.0, 1.0);
  float right = clamp(dir.x, 0.0, 1.0);
  float left = clamp(-dir.x, 0.0, 1.0);
  float down = clamp(dir.y, 0.0, 1.0);
  float inner_band = lens_step(s.bevel * -0.19, s.bevel * -0.024, dist);
  float k1 = lens_dim(1.0 - inner_band *
                                (up + left * 0.6 + right * 0.3 + down * 0.15) *
                                inner * 2.2);
  float deep_band = lens_step(s.bevel * -0.73, s.bevel * -0.13, dist);
  float k2 = lens_dim(1.0 - deep_band * left * inner * 2.0);
  float hxr = (dist + s.bevel * 0.45) / (s.bevel * 0.26);
  float ring = exp(-hxr * hxr);
  float k3 = lens_dim(1.0 - ring *
                                (right * 0.9 + up * 0.5 + left * 0.4 +
                                 down * 0.3) *
                                inner * 1.5);
  glass *= k1 * k2 * k3;

  // Rim light + fresnel, leaning towards the pointer.
  float rim = 0.55 * strength * lens.tune_rim;
  float fresnel = pow(clamp(1.0 - normal_z, 0.0, 1.0), 6.0);
  float rim_w = max(s.device_px * 1.1, 1.0) * max(lens.tune_rim_width, 0.05);
  float rim_line = 1.0 - lens_step(0.0, rim_w * 1.6, abs(dist + rim_w * 1.8));
  float fall = exp(-(local.y / 30.0) * (local.y / 30.0));
  float studio = right * right * fall * 0.9 + down * down * 0.2 +
                 left * left * 0.55;
  float2 light = float2(lens.light_x, lens.light_y);
  float2 light_dir =
      length(light) > 1e-3 ? normalize(light) : float2(0.0, -1.0);
  float light_cos = dot(dir, light_dir);
  float facing = clamp(light_cos, 0.0, 1.0);
  float away = clamp(-light_cos, 0.0, 1.0);
  float lit = facing * facing * 0.9 + away * away * 0.55;
  float lean = lens.light_mix * 0.4;
  float rim_dir = mix(studio, lit, lean);
  float front = mix(right, facing, lean);
  float back_side = mix(left, away, lean);
  float warm = 1.0 - lens_step(-0.5, 0.1, nx);
  float3 warm_cool = mix(float3(0.82, 0.9, 1.08), float3(1.2, 0.95, 0.78), warm);
  float3 rim_col = mix(float3(1.0), warm_cool, down * 0.6);
  rim_col = mix(rim_col, float3(0.12, 0.6, 1.2), back_side * 0.85);
  float lit_on = rim > 0.001 ? 1.0 : 0.0;
  float add = rim_line * rim_dir * rim +
              fresnel * front * 0.02 * lens.tune_fresnel * lit_on;
  // Caustic: warm light gathering inside the bottom edge.
  float caustic = lens_step(0.72, 0.98, depth) * down * 0.08 * strength *
                  lens.tune_caustic;
  glass += rim_col * add + float3(1.0, 0.92, 0.86) * caustic;

  float4 border_color = hsla_to_rgba(lens.border_color);
  float border_w = lens.border_width / max(lens.unit, 1e-3);
  if (border_w > 1e-4 && border_color.a > 0.0) {
    float t = lens_step(-border_w - aa, -border_w + aa, dist) * border_color.a;
    glass = mix(glass, border_color.rgb, t);
  }

  // The glass over its shadow, straight alpha.
  float3 g = clamp(glass, float3(0.0), float3(1.0));
  float a = mask + shadow_alpha * (1.0 - mask);
  if (a <= 1e-5) {
    discard_fragment();
  }
  return float4(g * mask / a, a * lens.opacity * mask_coverage);
}

struct LensDownsampleVertexOutput {
  uint slot_id [[flat]];
  float4 position [[position]];
};

struct LensDownsampleFragmentInput {
  uint slot_id [[flat]];
  float4 position [[position]];
};

vertex LensDownsampleVertexOutput lens_downsample_vertex(
    uint vertex_id [[vertex_id]], uint slot_id [[instance_id]],
    constant LensDownsampleSlot *slots [[buffer(LensDownsampleIndex_Slots)]],
    constant Size_DevicePixels *target_size
    [[buffer(LensDownsampleIndex_TargetSize)]]) {
  float2 unit_vertex = float2(float(vertex_id & 1u), 0.5 * float(vertex_id & 2u));
  LensDownsampleSlot slot = slots[slot_id];
  float2 position = float2(slot.dst_x, slot.dst_y) +
                    unit_vertex * float2(slot.dst_w, slot.dst_h);
  float2 size = float2((float)target_size->width, (float)target_size->height);
  float2 ndc = position / size * float2(2.0, -2.0) + float2(-1.0, 1.0);
  return LensDownsampleVertexOutput{slot_id, float4(ndc, 0.0, 1.0)};
}

// Dual-Kawase downsample: the centre and four diagonal taps one source
// texel away, each a bilinear average of four source texels, kept inside
// the slot's part of the source level.
fragment float4 lens_downsample_fragment(
    LensDownsampleFragmentInput input [[stage_in]],
    constant LensDownsampleSlot *slots [[buffer(LensDownsampleIndex_Slots)]],
    texture2d<float> source [[texture(LensDownsampleIndex_Source)]]) {
  constexpr sampler down_sampler(mag_filter::linear, min_filter::linear,
                                 address::clamp_to_edge);
  LensDownsampleSlot slot = slots[input.slot_id];
  float2 source_size = float2(source.get_width(), source.get_height());
  float2 lo = float2(slot.src_x0, slot.src_y0) + 0.5;
  float2 hi = max(float2(slot.src_x1, slot.src_y1) - 0.5, lo);
  float2 c = input.position.xy * 2.0;
  float4 sum = source.sample(down_sampler, clamp(c, lo, hi) / source_size) * 4.0;
  sum += source.sample(down_sampler, clamp(c + float2(1.0, 1.0), lo, hi) / source_size);
  sum += source.sample(down_sampler, clamp(c + float2(-1.0, 1.0), lo, hi) / source_size);
  sum += source.sample(down_sampler, clamp(c + float2(1.0, -1.0), lo, hi) / source_size);
  sum += source.sample(down_sampler, clamp(c + float2(-1.0, -1.0), lo, hi) / source_size);
  return sum / 8.0;
}

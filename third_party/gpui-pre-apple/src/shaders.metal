#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

float4 hsla_to_rgba(Hsla hsla);
float3 srgb_to_linear(float3 color);
float3 linear_to_srgb(float3 color);
float4 srgb_to_oklab(float4 color);
float4 oklab_to_srgb(float4 color);
float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *viewport_size);
float4 to_device_position_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          TransformationMatrix transformation,
                          constant Size_DevicePixels *input_viewport_size);

float2 to_tile_position(float2 unit_vertex, AtlasTile tile,
                        constant Size_DevicePixels *atlas_size);
float4 distance_from_clip_rect(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds);
float4 distance_from_clip_rect_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds, TransformationMatrix transformation);
float corner_dash_velocity(float dv1, float dv2);
float dash_alpha(float t, float period, float length, float dash_velocity,
                 float antialias_threshold);
float quarter_ellipse_sdf(float2 point, float2 radii);
float pick_corner_radius(float2 center_to_point, Corners_ScaledPixels corner_radii);
float quad_sdf(float2 point, Bounds_ScaledPixels bounds,
               Corners_ScaledPixels corner_radii);
float quad_sdf_impl(float2 center_to_point, float corner_radius);
float gaussian(float x, float sigma);
float2 erf(float2 x);
float blur_along_x(float x, float y, float sigma, float corner,
                   float2 half_size);
float4 over(float4 below, float4 above);
float radians(float degrees);
float4 fill_color(Background background, float2 position, Bounds_ScaledPixels bounds,
  float4 solid_color, float4 color0, float4 color1);

struct GradientColor {
  float4 solid;
  float4 color0;
  float4 color1;
};
GradientColor prepare_fill_color(uint tag, uint color_space, Hsla solid, Hsla color0, Hsla color1);

struct QuadVertexOutput {
  uint quad_id [[flat]];
  float4 position [[position]];
  float4 border_color [[flat]];
  float4 background_solid [[flat]];
  float4 background_color0 [[flat]];
  float4 background_color1 [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct QuadFragmentInput {
  uint quad_id [[flat]];
  float4 position [[position]];
  float4 border_color [[flat]];
  float4 background_solid [[flat]];
  float4 background_color0 [[flat]];
  float4 background_color1 [[flat]];
};

vertex QuadVertexOutput quad_vertex(uint unit_vertex_id [[vertex_id]],
                                    uint quad_id [[instance_id]],
                                    constant float2 *unit_vertices
                                    [[buffer(QuadInputIndex_Vertices)]],
                                    constant Quad *quads
                                    [[buffer(QuadInputIndex_Quads)]],
                                    constant Size_DevicePixels *viewport_size
                                    [[buffer(QuadInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Quad quad = quads[quad_id];
  float4 device_position =
      to_device_position(unit_vertex, quad.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, quad.bounds,
                                                 quad.content_mask.bounds);
  float4 border_color = hsla_to_rgba(quad.border_color);

  GradientColor gradient = prepare_fill_color(
    quad.background.tag,
    quad.background.color_space,
    quad.background.solid,
    quad.background.colors[0].color,
    quad.background.colors[1].color
  );

  return QuadVertexOutput{
      quad_id,
      device_position,
      border_color,
      gradient.solid,
      gradient.color0,
      gradient.color1,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 quad_fragment(QuadFragmentInput input [[stage_in]],
                              constant Quad *quads
                              [[buffer(QuadInputIndex_Quads)]]) {
  Quad quad = quads[input.quad_id];
  float4 background_color = fill_color(quad.background, input.position.xy, quad.bounds,
    input.background_solid, input.background_color0, input.background_color1);

  bool unrounded = quad.corner_radii.top_left == 0.0 &&
    quad.corner_radii.bottom_left == 0.0 &&
    quad.corner_radii.top_right == 0.0 &&
    quad.corner_radii.bottom_right == 0.0;

  // Fast path when the quad is not rounded and doesn't have any border
  if (quad.border_widths.top == 0.0 &&
      quad.border_widths.left == 0.0 &&
      quad.border_widths.right == 0.0 &&
      quad.border_widths.bottom == 0.0 &&
      unrounded) {
    return background_color;
  }

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
    return background_color;
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

  float4 color = background_color;
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

    // Blend the border on top of the background and then linearly interpolate
    // between the two as we slide inside the background.
    float4 blended_border = over(background_color, border_color);
    color = mix(background_color, blended_border,
                saturate(antialias_threshold - inner_sdf));
  }

  return color * float4(1.0, 1.0, 1.0, saturate(antialias_threshold - outer_sdf));
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

struct ShadowVertexOutput {
  float4 position [[position]];
  float4 color [[flat]];
  uint shadow_id [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct ShadowFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
  uint shadow_id [[flat]];
};

vertex ShadowVertexOutput shadow_vertex(
    uint unit_vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(ShadowInputIndex_Vertices)]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Shadow shadow = shadows[shadow_id];

  Bounds_ScaledPixels bounds;
  if (shadow.inset != 0u) {
    bounds = shadow.element_bounds;
  } else {
    // Leave room for the gaussian tail outside the shadow rect.
    float margin = 3. * shadow.blur_radius;
    bounds = shadow.bounds;
    bounds.origin.x -= margin;
    bounds.origin.y -= margin;
    bounds.size.width += 2. * margin;
    bounds.size.height += 2. * margin;
  }

  float4 device_position =
      to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, shadow.content_mask.bounds);
  float4 color = hsla_to_rgba(shadow.color);

  return ShadowVertexOutput{
      device_position,
      color,
      shadow_id,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 shadow_fragment(ShadowFragmentInput input [[stage_in]],
                                constant Shadow *shadows
                                [[buffer(ShadowInputIndex_Shadows)]]) {
  Shadow shadow = shadows[input.shadow_id];

  float2 origin = float2(shadow.bounds.origin.x, shadow.bounds.origin.y);
  float2 size = float2(shadow.bounds.size.width, shadow.bounds.size.height);
  float2 half_size = size / 2.;
  float2 center = origin + half_size;
  float2 point = input.position.xy - center;
  float corner_radius;
  if (point.x < 0.) {
    if (point.y < 0.) {
      corner_radius = shadow.corner_radii.top_left;
    } else {
      corner_radius = shadow.corner_radii.bottom_left;
    }
  } else {
    if (point.y < 0.) {
      corner_radius = shadow.corner_radii.top_right;
    } else {
      corner_radius = shadow.corner_radii.bottom_right;
    }
  }

  float alpha;
  if (shadow.blur_radius == 0.) {
    float distance = quad_sdf(input.position.xy, shadow.bounds, shadow.corner_radii);
    alpha = saturate(0.5 - distance);
  } else {
    // The signal is only non-zero in a limited range, so don't waste samples
    float low = point.y - half_size.y;
    float high = point.y + half_size.y;
    float start = clamp(-3. * shadow.blur_radius, low, high);
    float end = clamp(3. * shadow.blur_radius, low, high);

    // Accumulate samples (we can get away with surprisingly few samples)
    float step = (end - start) / 4.;
    float y = start + step * 0.5;
    alpha = 0.;
    for (int i = 0; i < 4; i++) {
      alpha += blur_along_x(point.x, point.y - y, shadow.blur_radius,
                            corner_radius, half_size) *
               gaussian(y, shadow.blur_radius) * step;
      y += step;
    }
  }

  if (shadow.inset != 0u) {
    // The inset shadow is the complement of the (blurred) hole rect, clipped to the element.
    // `saturate(0.5 - d)` gives a 1-pixel antialiased edge: d <= -0.5 -> 1, d >= 0.5 -> 0.
    alpha = 1. - alpha;
    float element_distance = quad_sdf(input.position.xy, shadow.element_bounds,
                                      shadow.element_corner_radii);
    alpha *= saturate(0.5 - element_distance);
  }

  return input.color * float4(1., 1., 1., alpha);
}

struct UnderlineVertexOutput {
  float4 position [[position]];
  float4 color [[flat]];
  uint underline_id [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct UnderlineFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
  uint underline_id [[flat]];
};

vertex UnderlineVertexOutput underline_vertex(
    uint unit_vertex_id [[vertex_id]], uint underline_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(UnderlineInputIndex_Vertices)]],
    constant Underline *underlines [[buffer(UnderlineInputIndex_Underlines)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Underline underline = underlines[underline_id];
  float4 device_position =
      to_device_position(unit_vertex, underline.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, underline.bounds,
                                                 underline.content_mask.bounds);
  float4 color = hsla_to_rgba(underline.color);
  return UnderlineVertexOutput{
      device_position,
      color,
      underline_id,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 underline_fragment(UnderlineFragmentInput input [[stage_in]],
                                   constant Underline *underlines
                                   [[buffer(UnderlineInputIndex_Underlines)]]) {
  const float WAVE_FREQUENCY = 2.0;
  const float WAVE_HEIGHT_RATIO = 0.8;

  Underline underline = underlines[input.underline_id];
  if (underline.wavy) {
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
    return input.color * float4(1., 1., 1., alpha);
  } else {
    return input.color;
  }
}

struct MonochromeSpriteVertexOutput {
  float4 position [[position]];
  float2 tile_position;
  float4 color [[flat]];
  float4 clip_distance;
};

struct MonochromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  float4 color [[flat]];
  float4 clip_distance;
};

vertex MonochromeSpriteVertexOutput monochrome_sprite_vertex(
    uint unit_vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
    constant MonochromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SpriteInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]],
    constant LensInkInstance *inks [[buffer(SpriteInputIndex_Inks)]],
    texture2d<float> lens_atlas [[texture(SpriteInputIndex_LensAtlas)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
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
  if (sprite.pad > 0) {
    LensInkInstance ink = inks[sprite.pad - 1];
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
  return MonochromeSpriteVertexOutput{
      device_position,
      tile_position,
      color,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
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
  return color;
}

struct PolychromeSpriteVertexOutput {
  float4 position [[position]];
  float2 tile_position;
  uint sprite_id [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct PolychromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  uint sprite_id [[flat]];
};

vertex PolychromeSpriteVertexOutput polychrome_sprite_vertex(
    uint unit_vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SpriteInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]]) {

  float2 unit_vertex = unit_vertices[unit_vertex_id];
  PolychromeSprite sprite = sprites[sprite_id];
  float4 device_position =
      to_device_position(unit_vertex, sprite.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, sprite.bounds,
                                                 sprite.content_mask.bounds);
  float2 tile_position = to_tile_position(unit_vertex, sprite.tile, atlas_size);
  return PolychromeSpriteVertexOutput{
      device_position,
      tile_position,
      sprite_id,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 polychrome_sprite_fragment(
    PolychromeSpriteFragmentInput input [[stage_in]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    texture2d<float> atlas_texture [[texture(SpriteInputIndex_AtlasTexture)]]) {
  PolychromeSprite sprite = sprites[input.sprite_id];
  constexpr sampler atlas_texture_sampler(mag_filter::linear,
                                          min_filter::linear);
  float4 sample =
      atlas_texture.sample(atlas_texture_sampler, input.tile_position);
  float distance =
      quad_sdf(input.position.xy, sprite.bounds, sprite.corner_radii);

  float4 color = sample;
  if (sprite.grayscale) {
    float grayscale = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
    color.r = grayscale;
    color.g = grayscale;
    color.b = grayscale;
  }
  color.a *= sprite.opacity * saturate(0.5 - distance);
  return color;
}

struct PathRasterizationVertexOutput {
  float4 position [[position]];
  float2 st_position;
  uint vertex_id [[flat]];
  float clip_rect_distance [[clip_distance]][4];
};

struct PathRasterizationFragmentInput {
  float4 position [[position]];
  float2 st_position;
  uint vertex_id [[flat]];
};

vertex PathRasterizationVertexOutput path_rasterization_vertex(
  uint vertex_id [[vertex_id]],
  constant PathRasterizationVertex *vertices [[buffer(PathRasterizationInputIndex_Vertices)]],
  constant Size_DevicePixels *atlas_size [[buffer(PathRasterizationInputIndex_ViewportSize)]]
) {
  PathRasterizationVertex v = vertices[vertex_id];
  float2 vertex_position = float2(v.xy_position.x, v.xy_position.y);
  float4 position = float4(
    vertex_position * float2(2. / atlas_size->width, -2. / atlas_size->height) + float2(-1., 1.),
    0.,
    1.
  );
  return PathRasterizationVertexOutput{
      position,
      float2(v.st_position.x, v.st_position.y),
      vertex_id,
      {
        v.xy_position.x - v.bounds.origin.x,
        v.bounds.origin.x + v.bounds.size.width - v.xy_position.x,
        v.xy_position.y - v.bounds.origin.y,
        v.bounds.origin.y + v.bounds.size.height - v.xy_position.y
      }
  };
}

fragment float4 path_rasterization_fragment(
  PathRasterizationFragmentInput input [[stage_in]],
  constant PathRasterizationVertex *vertices [[buffer(PathRasterizationInputIndex_Vertices)]]
) {
  float2 dx = dfdx(input.st_position);
  float2 dy = dfdy(input.st_position);

  PathRasterizationVertex v = vertices[input.vertex_id];
  Background background = v.color;
  Bounds_ScaledPixels path_bounds = v.bounds;
  float alpha;
  if (length(float2(dx.x, dy.x)) < 0.001) {
    alpha = 1.0;
  } else {
    float2 gradient = float2(
      (2. * input.st_position.x) * dx.x - dx.y,
      (2. * input.st_position.x) * dy.x - dy.y
    );
    float f = (input.st_position.x * input.st_position.x) - input.st_position.y;
    float distance = f / length(gradient);
    alpha = saturate(0.5 - distance);
  }

  GradientColor gradient_color = prepare_fill_color(
    background.tag,
    background.color_space,
    background.solid,
    background.colors[0].color,
    background.colors[1].color
  );

  float4 color = fill_color(
    background,
    input.position.xy,
    path_bounds,
    gradient_color.solid,
    gradient_color.color0,
    gradient_color.color1
  );
  return float4(color.rgb * color.a * alpha, alpha * color.a);
}

struct PathSpriteVertexOutput {
  float4 position [[position]];
  float2 texture_coords;
};

vertex PathSpriteVertexOutput path_sprite_vertex(
  uint unit_vertex_id [[vertex_id]],
  uint sprite_id [[instance_id]],
  constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
  constant PathSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
  constant Size_DevicePixels *viewport_size [[buffer(SpriteInputIndex_ViewportSize)]]
) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  PathSprite sprite = sprites[sprite_id];
  // Don't apply content mask because it was already accounted for when
  // rasterizing the path.
  float4 device_position =
      to_device_position(unit_vertex, sprite.bounds, viewport_size);

  float2 screen_position = float2(sprite.bounds.origin.x, sprite.bounds.origin.y) + unit_vertex * float2(sprite.bounds.size.width, sprite.bounds.size.height);
  float2 texture_coords = screen_position / float2(viewport_size->width, viewport_size->height);

  return PathSpriteVertexOutput{
    device_position,
    texture_coords
  };
}

fragment float4 path_sprite_fragment(
  PathSpriteVertexOutput input [[stage_in]],
  texture2d<float> intermediate_texture [[texture(SpriteInputIndex_AtlasTexture)]]
) {
  constexpr sampler intermediate_texture_sampler(mag_filter::linear, min_filter::linear);
  return intermediate_texture.sample(intermediate_texture_sampler, input.texture_coords);
}

struct SurfaceVertexOutput {
  float4 position [[position]];
  float2 texture_position;
  float clip_distance [[clip_distance]][4];
};

struct SurfaceFragmentInput {
  float4 position [[position]];
  float2 texture_position;
};

vertex SurfaceVertexOutput surface_vertex(
    uint unit_vertex_id [[vertex_id]], uint surface_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SurfaceInputIndex_Vertices)]],
    constant SurfaceBounds *surfaces [[buffer(SurfaceInputIndex_Surfaces)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SurfaceInputIndex_ViewportSize)]],
    constant Size_DevicePixels *texture_size
    [[buffer(SurfaceInputIndex_TextureSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  SurfaceBounds surface = surfaces[surface_id];
  float4 device_position =
      to_device_position(unit_vertex, surface.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, surface.bounds,
                                                 surface.content_mask.bounds);
  // We are going to copy the whole texture, so the texture position corresponds
  // to the current vertex of the unit triangle.
  float2 texture_position = unit_vertex;
  return SurfaceVertexOutput{
      device_position,
      texture_position,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
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

  return ycbcrToRGBTransform * ycbcr;
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
                alpha);
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

float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *input_viewport_size) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  float2 viewport_size = float2((float)input_viewport_size->width,
                                (float)input_viewport_size->height);
  float2 device_position =
      position / viewport_size * float2(2., -2.) + float2(-1., 1.);
  return float4(device_position, 0., 1.);
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

// A standard gaussian function, used for weighting samples
float gaussian(float x, float sigma) {
  return exp(-(x * x) / (2. * sigma * sigma)) / (sqrt(2. * M_PI_F) * sigma);
}

// This approximates the error function, needed for the gaussian integral
float2 erf(float2 x) {
  float2 s = sign(x);
  float2 a = abs(x);
  float2 r1 = 1. + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
  float2 r2 = r1 * r1;
  return s - s / (r2 * r2);
}

float blur_along_x(float x, float y, float sigma, float corner,
                   float2 half_size) {
  float delta = min(half_size.y - corner - abs(y), 0.);
  float curved =
      half_size.x - corner + sqrt(max(0., corner * corner - delta * delta));
  float2 integral =
      0.5 + 0.5 * erf((x + float2(-curved, curved)) * (sqrt(0.5) / sigma));
  return integral.y - integral.x;
}

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
  float4 result;
  float alpha = above.a + below.a * (1.0 - above.a);
  result.rgb =
      (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / alpha;
  result.a = alpha;
  return result;
}

GradientColor prepare_fill_color(uint tag, uint color_space, Hsla solid,
                                     Hsla color0, Hsla color1) {
  GradientColor out;
  if (tag == 0 || tag == 2 || tag == 3) {
    out.solid = hsla_to_rgba(solid);
  } else if (tag == 1) {
    out.color0 = hsla_to_rgba(color0);
    out.color1 = hsla_to_rgba(color1);

    // Prepare color space in vertex for avoid conversion
    // in fragment shader for performance reasons
    if (color_space == 1) {
      // Oklab
      out.color0 = srgb_to_oklab(out.color0);
      out.color1 = srgb_to_oklab(out.color1);
    }
  }

  return out;
}

float2x2 rotate2d(float angle) {
    float s = sin(angle);
    float c = cos(angle);
    return float2x2(c, -s, s, c);
}

float4 fill_color(Background background,
                      float2 position,
                      Bounds_ScaledPixels bounds,
                      float4 solid_color, float4 color0, float4 color1) {
  float4 color;

  switch (background.tag) {
    case 0:
      color = solid_color;
      break;
    case 1: {
      // -90 degrees to match the CSS gradient angle.
      float gradient_angle = background.gradient_angle_or_pattern_height;
      float radians = (fmod(gradient_angle, 360.0) - 90.0) * (M_PI_F / 180.0);
      float2 direction = float2(cos(radians), sin(radians));

      // Expand the short side to be the same as the long side
      if (bounds.size.width > bounds.size.height) {
          direction.y *= bounds.size.height / bounds.size.width;
      } else {
          direction.x *=  bounds.size.width / bounds.size.height;
      }

      // Get the t value for the linear gradient with the color stop percentages.
      float2 half_size = float2(bounds.size.width, bounds.size.height) / 2.;
      float2 center = float2(bounds.origin.x, bounds.origin.y) + half_size;
      float2 center_to_point = position - center;
      float t = dot(center_to_point, direction) / length(direction);
      // Check the direction to determine whether to use x or y
      if (abs(direction.x) > abs(direction.y)) {
          t = (t + half_size.x) / bounds.size.width;
      } else {
          t = (t + half_size.y) / bounds.size.height;
      }

      // Adjust t based on the stop percentages
      t = (t - background.colors[0].percentage)
        / (background.colors[1].percentage
        - background.colors[0].percentage);
      t = clamp(t, 0.0, 1.0);

      switch (background.color_space) {
        case 0:
          color = mix(color0, color1, t);
          break;
        case 1: {
          float4 oklab_color = mix(color0, color1, t);
          color = oklab_to_srgb(oklab_color);
          break;
        }
      }

      // Dither to reduce banding in gradients (especially dark/alpha).
      // Triangular-distributed noise breaks up 8-bit quantization steps.
      // ±2/255 for RGB (enough for dark-on-dark compositing),
      // ±3/255 for alpha (needs more because alpha × dark color = tiny steps).
      {
        float2 seed = position * 0.6180339887; // golden ratio spread
        float r1 = fract(sin(dot(seed, float2(12.9898, 78.233))) * 43758.5453);
        float r2 = fract(sin(dot(seed, float2(39.3460, 11.135))) * 24634.6345);
        float tri = r1 + r2 - 1.0; // triangular PDF, range [-1, +1]
        color.rgb += tri * 2.0 / 255.0;
        color.a   += tri * 3.0 / 255.0;
      }

      break;
    }
    case 2: {
        float gradient_angle_or_pattern_height = background.gradient_angle_or_pattern_height;
        float pattern_width = (gradient_angle_or_pattern_height / 65535.0f) / 255.0f;
        float pattern_interval = fmod(gradient_angle_or_pattern_height, 65535.0f) / 255.0f;
        float pattern_height = pattern_width + pattern_interval;
        float stripe_angle = M_PI_F / 4.0;
        float pattern_period = pattern_height * sin(stripe_angle);
        float2x2 rotation = rotate2d(stripe_angle);
        float2 relative_position = position - float2(bounds.origin.x, bounds.origin.y);
        float2 rotated_point = rotation * relative_position;
        float pattern = fmod(rotated_point.x, pattern_period);
        float distance = min(pattern, pattern_period - pattern) - pattern_period * (pattern_width / pattern_height) /  2.0f;
        color = solid_color;
        color.a *= saturate(0.5 - distance);
        break;
    }
    case 3: {
        // checkerboard
        float size = background.gradient_angle_or_pattern_height;
        float2 relative_position = position - float2(bounds.origin.x, bounds.origin.y);

        float x_index = floor(relative_position.x / size);
        float y_index = floor(relative_position.y / size);
        float should_be_colored = fmod(x_index + y_index, 2.0);

        color = solid_color;
        color.a *= saturate(should_be_colored);
        break;
    }
  }

  return color;
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

vertex LensVertexOutput lens_vertex(uint unit_vertex_id [[vertex_id]],
                                    uint lens_id [[instance_id]],
                                    constant float2 *unit_vertices
                                    [[buffer(LensInputIndex_Vertices)]],
                                    constant BackdropLens *lenses
                                    [[buffer(LensInputIndex_Lenses)]],
                                    constant Size_DevicePixels *viewport_size
                                    [[buffer(LensInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
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
  float soft = lens.shadow_softness;
  float off = lens.shadow_offset;
  float near = 1.0 - lens_step(-6.0, soft, lens_sdf(s, local - float2(0.0, off)));
  float far = 1.0 - lens_step(-10.0, soft * 1.5,
                              lens_sdf(s, local - float2(0.0, off * 1.6)));
  float aa = max(s.device_px * 1.2, 1e-4);
  float mask = 1.0 - lens_step(-aa, aa, dist);
  float2 in_footprint =
      input.position.xy - float2(lens.bounds.origin.x, lens.bounds.origin.y);
  float border = min(min(in_footprint.x, in_footprint.y),
                     min(lens.bounds.size.width - in_footprint.x,
                         lens.bounds.size.height - in_footprint.y));
  float sh = clamp((near + far) * lens.shadow * strength * v_fade * outside,
                   0.0, 0.85) *
             lens_step(0.0, 14.0, border);
  float shadow_alpha = 1.0 - lens_dim(1.0 - sh);
  if (mask <= 0.0) {
    if (shadow_alpha <= 0.0) {
      discard_fragment();
    }
    return float4(0.0, 0.0, 0.0, shadow_alpha * lens.opacity);
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
    glass = mix(surface.rgb, color.rgb, 1.0 - edge * lens.edge_clear);
  } else if (lens.bend == 1) {
    float amount = lens.radial_amount / max(lens.unit, 1e-3);
    float2 bend = lens_radial_bend(s, lens, local, amount);
    glass = lens_bent(s, slot, atlas, atlas_size, local, bend, 0.0,
                      max(s.blur_shift, 0.0));
    if (frost_sigma > 0.0) {
      float ratio = clamp(1.0 - abs(dist) / (min(s.half_size.x, s.half_size.y) *
                                             s.edge_band / 0.35),
                          0.0, 1.0);
      float sigma = frost_sigma * mix(1.0, s.frost_edge, pow(ratio, 1.3));
      float3 frost = lens_frosted(s, slot, atlas, atlas_size, local + bend,
                                  max(sigma, 0.5));
      glass = mix(glass, frost, lens.blur_mix);
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
    glass = lens_bent(s, slot, atlas, atlas_size, at, bend, split, lod);
    if (frost_sigma > 0.0) {
      float sigma =
          frost_sigma * mix(1.0, s.frost_edge, pow(bend_edge, 1.3));
      float3 frost = lens_frosted(s, slot, atlas, atlas_size, at + bend,
                                  max(sigma, 0.5));
      glass = mix(glass, frost, lens.blur_mix);
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

  // Frosted resting state.
  if (clear) {
    glass = mix(glass, color.rgb, lens.frost);
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
  return float4(g * mask / a, a * lens.opacity);
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
    uint unit_vertex_id [[vertex_id]], uint slot_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(LensDownsampleIndex_Vertices)]],
    constant LensDownsampleSlot *slots [[buffer(LensDownsampleIndex_Slots)]],
    constant Size_DevicePixels *target_size
    [[buffer(LensDownsampleIndex_TargetSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
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

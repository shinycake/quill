//! IEEE 754 binary16 ("half float") conversions for HDR pixels.

/// binary16 to f32.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(super) fn half_to_f32(bits: u16) -> f32 {
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = f32::from(bits & 0x3ff);
    let magnitude = match exponent {
        0 => mantissa * 2.0_f32.powi(-24),
        0x1f if mantissa == 0.0 => f32::INFINITY,
        0x1f => f32::NAN,
        _ => (1.0 + mantissa / 1024.0) * 2.0_f32.powi(i32::from(exponent) - 15),
    };
    if bits & 0x8000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// f32 to binary16, rounding to nearest (ties to even). Values past the
/// half range become infinity, NaN stays NaN.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(super) fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x007f_ffff;
    if exponent == 0xff {
        let nan = if mantissa != 0 { 0x0200 } else { 0 };
        return sign | 0x7c00 | nan;
    }
    // Rebias from f32's 127 to binary16's 15.
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        // Subnormal (or zero): shift the full significand into place.
        if half_exponent < -10 {
            return sign;
        }
        let significand = mantissa | 0x0080_0000;
        let shift = (14 - half_exponent) as u32;
        let half = significand >> shift;
        let remainder = significand & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let round_up = remainder > halfway || (remainder == halfway && half & 1 == 1);
        return sign | (half + u32::from(round_up)) as u16;
    }
    let half = ((half_exponent as u32) << 10) | (mantissa >> 13);
    let remainder = mantissa & 0x1fff;
    let round_up = remainder > 0x1000 || (remainder == 0x1000 && half & 1 == 1);
    // A carry out of the mantissa bumps the exponent, up to infinity.
    sign | (half + u32::from(round_up)) as u16
}

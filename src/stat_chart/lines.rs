//! The legend's line switches (tdesktop `ChartLinesFilterWidget`).

/// Switch line `index` on or off from the legend. The last shown line
/// stays on (tdesktop shakes its switch); `false` when refused.
pub fn toggle_line(enabled: &mut [bool], index: usize) -> bool {
    let Some(&on) = enabled.get(index) else {
        return false;
    };
    if on && enabled.iter().filter(|e| **e).count() <= 1 {
        return false;
    }
    if let Some(slot) = enabled.get_mut(index) {
        *slot = !on;
    }
    true
}

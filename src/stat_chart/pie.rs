//! A percentage chart zoomed into a few days becomes a pie of the shares
//! over the selected days (tdesktop `StackLinearChartView::maybeLocalZoom`,
//! `PiePartsPercentage`).

/// One slice of the pie.
#[derive(Debug, Clone, PartialEq)]
pub struct PiePart {
    /// Share of the whole, 0..=1.
    pub share: f64,
    /// `42%`.
    pub text: String,
}

/// Days around the zoomed point that the pie's slider then covers.
const LIMIT_LENGTH: usize = 8;

/// Shares of `sums` (`PiePartsPercentage`). With `round`, each share is
/// rounded to a whole percent and the rounding error goes to the part
/// rounded the most, so the labels add up to 100%.
pub fn pie_parts(sums: &[f64], round: bool) -> Vec<PiePart> {
    let total: f64 = sums.iter().sum();
    let mut shares = Vec::with_capacity(sums.len());
    let mut diff_sum = 0.0;
    let (mut max_diff, mut max_index) = (0.0, None);
    for (k, sum) in sums.iter().enumerate() {
        let raw = if total > 0.0 { sum / total } else { 0.0 };
        let share = if round {
            (raw * 100.0).round() * 0.01
        } else {
            raw
        };
        diff_sum += raw - share;
        if (raw - share).abs() > max_diff {
            max_diff = (raw - share).abs();
            max_index = Some(k);
        }
        shares.push(share);
    }
    // tdesktop's min and max trackers both keep the largest error, so
    // the correction always lands on that part.
    if round && let Some(index) = max_index {
        shares[index] += diff_sum;
    }
    shares
        .into_iter()
        .map(|share| PiePart {
            share,
            text: format!("{}%", (share * 100.0 + 1e-9) as i64),
        })
        .collect()
}

/// The days a percentage chart's pie zooms over around point `index`
/// of `len` (tdesktop `processLocalZoom`: eight days, the clicked one
/// in the middle where the data allows), as inclusive x indices.
pub fn local_zoom_window(len: usize, index: usize) -> (usize, usize) {
    let Some(back) = len.checked_sub(1) else {
        return (0, 0);
    };
    let index = index.min(back);
    let (left_side, right_side) = (LIMIT_LENGTH / 2, LIMIT_LENGTH / 2 + 1);
    if index < left_side {
        (0, (left_side + right_side).min(back))
    } else if index + right_side > back {
        (back.saturating_sub(LIMIT_LENGTH), back)
    } else {
        (index - left_side, index + right_side)
    }
}

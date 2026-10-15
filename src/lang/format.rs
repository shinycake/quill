//! `{tag}` substitution and the count formats tdesktop's `lt_count`,
//! `lt_count_decimal` and `lt_count_short` produce.

/// Replaces each `{name}` whose name is in `tags`; others stay literal.
pub(crate) fn substitute(template: &str, tags: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let (before, from_open) = rest.split_at(open);
        out.push_str(before);
        let Some(close) = from_open.find('}') else {
            out.push_str(from_open);
            return out;
        };
        let (tag, after) = from_open.split_at(close + 1);
        let name = tag.trim_start_matches('{').trim_end_matches('}');
        match tags.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(tag),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `1234567` → `1,234,567`.
pub(crate) fn group_digits(count: i64) -> String {
    let digits = count.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if count < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `9999` → `9999`, `12345` → `12.3K`, `2500000` → `2.5M`, `3000000000` →
/// `3B` (one truncated decimal, dropped when zero), as tdesktop's
/// `lt_count_short`.
pub(crate) fn short_count(count: i64) -> String {
    let abs = count.unsigned_abs();
    let (divider, suffix) = if abs >= 1_000_000_000 {
        (1_000_000_000, "B")
    } else if abs >= 1_000_000 {
        (1_000_000, "M")
    } else if abs >= 10_000 {
        (1_000, "K")
    } else {
        return count.to_string();
    };
    let tenths = abs / (divider / 10);
    let sign = if count < 0 { "-" } else { "" };
    let (whole, tenth) = (tenths / 10, tenths % 10);
    if tenth == 0 {
        format!("{sign}{whole}{suffix}")
    } else {
        format!("{sign}{whole}.{tenth}{suffix}")
    }
}

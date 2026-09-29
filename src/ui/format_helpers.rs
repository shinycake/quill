/// Phase 9.5: current Unix timestamp (seconds) — stealth active /
/// cooldown predicates and relative viewer times compare against this.
pub(crate) fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Phase C2h: "in 3h" / "in 2d 4h" countdown for a scheduled video
/// chat — relative only, no timezone math.
pub(crate) fn format_starts_in(start_date: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut secs = (start_date - now).max(0);
    if secs < 3600 {
        return format!("in {}m", (secs / 60).max(1));
    }
    secs /= 3600;
    if secs < 48 {
        format!("in {}h", secs)
    } else {
        format!("in {}d {}h", secs / 24, secs % 24)
    }
}

/// Phase C2h: mm:ss / h:mm:ss for the recording indicator.
pub(crate) fn format_record_duration(secs: i32) -> String {
    let secs = secs.max(0);
    if secs < 3600 {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    } else {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    }
}

/// Slice A3: relative "last active" for a session unix timestamp
/// (TGX `SessionLastActiveDate`) — relative only, no timezone math,
/// the `format_starts_in` precedent.
pub(crate) fn format_session_last_active(last_active_date: i32) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs = (now - last_active_date as i64).max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

//! B13: Settings → Data and Storage → Network usage. TDLib counts the
//! bytes it sent and received per network type and file type
//! (`getNetworkStatistics`, schema 1.8.67) and calls separately; this
//! folds the entries into Mobile / Wi-Fi / Roaming / Other groups with
//! the category rows Telegram X shows (`TGNetworkStats`). tdesktop has no
//! such screen, so the layout follows the TGX reference.

use serde_json::Value;

/// The network a group of entries was counted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetworkGroup {
    Mobile,
    WiFi,
    Roaming,
    Other,
}

impl NetworkGroup {
    pub const ALL: [NetworkGroup; 4] = [
        NetworkGroup::Mobile,
        NetworkGroup::WiFi,
        NetworkGroup::Roaming,
        NetworkGroup::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NetworkGroup::Mobile => "Mobile network",
            NetworkGroup::WiFi => "Wi-Fi",
            NetworkGroup::Roaming => "Roaming",
            NetworkGroup::Other => "Other networks",
        }
    }

    fn from_td(name: &str) -> Option<Self> {
        match name {
            "networkTypeMobile" => Some(Self::Mobile),
            "networkTypeWiFi" => Some(Self::WiFi),
            "networkTypeMobileRoaming" => Some(Self::Roaming),
            "networkTypeOther" => Some(Self::Other),
            _ => None,
        }
    }
}

/// One category row, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UsageKind {
    Photos,
    Videos,
    Animations,
    Files,
    Music,
    Voice,
    VideoNotes,
    Stickers,
    OtherMedia,
    Messages,
    Secret,
    Calls,
}

impl UsageKind {
    pub const ALL: [UsageKind; 12] = [
        UsageKind::Photos,
        UsageKind::Videos,
        UsageKind::Animations,
        UsageKind::Files,
        UsageKind::Music,
        UsageKind::Voice,
        UsageKind::VideoNotes,
        UsageKind::Stickers,
        UsageKind::OtherMedia,
        UsageKind::Messages,
        UsageKind::Secret,
        UsageKind::Calls,
    ];

    pub fn label(self) -> &'static str {
        match self {
            UsageKind::Photos => "Photos",
            UsageKind::Videos => "Videos",
            UsageKind::Animations => "GIFs",
            UsageKind::Files => "Files",
            UsageKind::Music => "Music",
            UsageKind::Voice => "Voice messages",
            UsageKind::VideoNotes => "Video messages",
            UsageKind::Stickers => "Stickers",
            UsageKind::OtherMedia => "Other media",
            UsageKind::Messages => "Messages and other data",
            UsageKind::Secret => "Secret chats",
            UsageKind::Calls => "Calls",
        }
    }

    fn from_file_type(name: &str) -> Self {
        match name {
            "fileTypeNone" => UsageKind::Messages,
            "fileTypeSecret" | "fileTypeSecretThumbnail" => UsageKind::Secret,
            "fileTypeSticker" => UsageKind::Stickers,
            "fileTypePhoto" | "fileTypePhotoStory" => UsageKind::Photos,
            "fileTypeVoiceNote" => UsageKind::Voice,
            "fileTypeVideo" | "fileTypeVideoStory" => UsageKind::Videos,
            "fileTypeVideoNote" => UsageKind::VideoNotes,
            "fileTypeDocument" => UsageKind::Files,
            "fileTypeAudio" => UsageKind::Music,
            "fileTypeAnimation" => UsageKind::Animations,
            _ => UsageKind::OtherMedia,
        }
    }
}

/// Sent / received bytes of one row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Traffic {
    pub sent: i64,
    pub received: i64,
}

impl Traffic {
    pub fn total(self) -> i64 {
        self.sent.saturating_add(self.received)
    }

    fn add(&mut self, other: Traffic) {
        self.sent = self.sent.saturating_add(other.sent);
        self.received = self.received.saturating_add(other.received);
    }
}

/// A parsed `networkStatistics` answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkUsage {
    /// Unix time counting started (the last reset).
    pub since_date: i64,
    cells: Vec<(NetworkGroup, UsageKind, Traffic)>,
    /// Total call time in seconds per group.
    call_seconds: Vec<(NetworkGroup, i64)>,
}

impl NetworkUsage {
    /// Parse a `networkStatistics` object; unknown network or entry
    /// types (`networkTypeNone`) are skipped like TGX does.
    pub fn from_value(value: &Value) -> Self {
        let mut usage = NetworkUsage {
            since_date: value.get("since_date").and_then(Value::as_i64).unwrap_or(0),
            ..Default::default()
        };
        let entries = value.get("entries").and_then(Value::as_array);
        for entry in entries.into_iter().flatten() {
            let Some(group) = entry
                .get("network_type")
                .and_then(|n| n.get("@type"))
                .and_then(Value::as_str)
                .and_then(NetworkGroup::from_td)
            else {
                continue;
            };
            let number = |key: &str| {
                entry
                    .get(key)
                    .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                    .unwrap_or(0)
                    .max(0)
            };
            let traffic = Traffic {
                sent: number("sent_bytes"),
                received: number("received_bytes"),
            };
            match entry.get("@type").and_then(Value::as_str) {
                Some("networkStatisticsEntryFile") => {
                    let kind = UsageKind::from_file_type(
                        entry
                            .get("file_type")
                            .and_then(|t| t.get("@type"))
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    );
                    usage.add(group, kind, traffic);
                }
                Some("networkStatisticsEntryCall") => {
                    usage.add(group, UsageKind::Calls, traffic);
                    let seconds = entry
                        .get("duration")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .max(0.0) as i64;
                    match usage.call_seconds.iter_mut().find(|(g, _)| *g == group) {
                        Some((_, total)) => *total += seconds,
                        None => usage.call_seconds.push((group, seconds)),
                    }
                }
                _ => {}
            }
        }
        usage
    }

    fn add(&mut self, group: NetworkGroup, kind: UsageKind, traffic: Traffic) {
        match self
            .cells
            .iter_mut()
            .find(|(g, k, _)| *g == group && *k == kind)
        {
            Some((_, _, cell)) => cell.add(traffic),
            None => self.cells.push((group, kind, traffic)),
        }
    }

    /// Traffic of one row on one network.
    pub fn cell(&self, group: NetworkGroup, kind: UsageKind) -> Traffic {
        self.cells
            .iter()
            .find(|(g, k, _)| *g == group && *k == kind)
            .map(|(_, _, t)| *t)
            .unwrap_or_default()
    }

    /// Traffic of one network (every row).
    pub fn group_total(&self, group: NetworkGroup) -> Traffic {
        let mut total = Traffic::default();
        for (g, _, traffic) in &self.cells {
            if *g == group {
                total.add(*traffic);
            }
        }
        total
    }

    /// Traffic over every network.
    pub fn grand_total(&self) -> Traffic {
        let mut total = Traffic::default();
        for group in NetworkGroup::ALL {
            total.add(self.group_total(group));
        }
        total
    }

    /// Call time on a network, in seconds.
    pub fn call_seconds(&self, group: NetworkGroup) -> i64 {
        self.call_seconds
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, s)| *s)
            .unwrap_or(0)
    }

    /// Non-empty rows of a network in display order.
    pub fn rows(&self, group: NetworkGroup) -> Vec<(UsageKind, Traffic)> {
        UsageKind::ALL
            .into_iter()
            .map(|kind| (kind, self.cell(group, kind)))
            .filter(|(_, traffic)| traffic.total() > 0)
            .collect()
    }

    /// Networks that counted anything.
    pub fn active_groups(&self) -> Vec<NetworkGroup> {
        NetworkGroup::ALL
            .into_iter()
            .filter(|g| self.group_total(*g).total() > 0 || self.call_seconds(*g) > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> NetworkUsage {
        NetworkUsage::from_value(&json!({
            "@type": "networkStatistics",
            "since_date": 1_700_000_000,
            "entries": [
                {"@type": "networkStatisticsEntryFile", "file_type": {"@type": "fileTypePhoto"},
                 "network_type": {"@type": "networkTypeWiFi"}, "sent_bytes": 10, "received_bytes": 1000},
                {"@type": "networkStatisticsEntryFile", "file_type": {"@type": "fileTypePhotoStory"},
                 "network_type": {"@type": "networkTypeWiFi"}, "sent_bytes": 0, "received_bytes": 500},
                {"@type": "networkStatisticsEntryFile", "file_type": {"@type": "fileTypeNone"},
                 "network_type": {"@type": "networkTypeMobile"}, "sent_bytes": 7, "received_bytes": 9},
                {"@type": "networkStatisticsEntryFile", "file_type": {"@type": "fileTypeWallpaper"},
                 "network_type": {"@type": "networkTypeNone"}, "sent_bytes": 99, "received_bytes": 99},
                {"@type": "networkStatisticsEntryCall", "network_type": {"@type": "networkTypeWiFi"},
                 "sent_bytes": 100, "received_bytes": 200, "duration": 61.5}
            ]
        }))
    }

    #[test]
    fn entries_fold_into_groups_and_rows() {
        let usage = sample();
        assert_eq!(usage.since_date, 1_700_000_000);
        // The two photo kinds fold together; `networkTypeNone` is dropped.
        assert_eq!(
            usage.cell(NetworkGroup::WiFi, UsageKind::Photos),
            Traffic {
                sent: 10,
                received: 1500
            }
        );
        assert_eq!(
            usage.group_total(NetworkGroup::WiFi).total(),
            10 + 1500 + 300
        );
        assert_eq!(usage.group_total(NetworkGroup::Mobile).total(), 16);
        assert_eq!(usage.grand_total().total(), 1810 + 16);
        assert_eq!(usage.call_seconds(NetworkGroup::WiFi), 61);
        assert_eq!(
            usage.active_groups(),
            [NetworkGroup::Mobile, NetworkGroup::WiFi]
        );
        let rows: Vec<UsageKind> = usage
            .rows(NetworkGroup::WiFi)
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        assert_eq!(rows, [UsageKind::Photos, UsageKind::Calls]);
    }

    #[test]
    fn int64_strings_and_empty_answers_parse() {
        let usage = NetworkUsage::from_value(&json!({
            "since_date": 5,
            "entries": [{"@type": "networkStatisticsEntryFile",
                "file_type": {"@type": "fileTypeVideo"},
                "network_type": {"@type": "networkTypeMobileRoaming"},
                "sent_bytes": "12", "received_bytes": "30"}]
        }));
        assert_eq!(
            usage.cell(NetworkGroup::Roaming, UsageKind::Videos).total(),
            42
        );
        let empty = NetworkUsage::from_value(&json!({}));
        assert!(empty.active_groups().is_empty());
        assert_eq!(empty.grand_total().total(), 0);
    }
}

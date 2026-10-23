//! The `supergroupFullInfo` fields the group and channel profile shows:
//! the location row of a location-based group and whether gifts can be
//! sent to the channel (tdesktop `CanSendStarGiftTo`).

use crate::business_info::{BusinessLocation, parse_location};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SupergroupProfileFacts {
    /// `location`: the address and point of a location-based group.
    pub location: Option<BusinessLocation>,
    /// `can_send_gift`.
    pub can_send_gift: bool,
}

impl SupergroupProfileFacts {
    pub(crate) fn parse(info: Option<&Value>) -> Self {
        Self {
            location: parse_location(info.and_then(|info| info.get("location"))),
            can_send_gift: info
                .and_then(|info| info.get("can_send_gift"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SupergroupProfileFacts;
    use serde_json::json;

    #[test]
    fn reads_the_location_and_the_gift_flag() {
        let info = json!({
            "can_send_gift": true,
            "location": {
                "@type": "chatLocation",
                "location": {"latitude": 51.5, "longitude": -0.12},
                "address": "London"
            }
        });
        let facts = SupergroupProfileFacts::parse(Some(&info));
        assert!(facts.can_send_gift);
        let location = facts.location.unwrap_or_default();
        assert_eq!(location.address, "London");
        assert_eq!(location.point_e6, Some((51_500_000, -120_000)));
    }

    #[test]
    fn missing_fields_mean_no_location_and_no_gifts() {
        assert_eq!(
            SupergroupProfileFacts::parse(Some(&json!({"location": null}))),
            SupergroupProfileFacts::default()
        );
    }
}

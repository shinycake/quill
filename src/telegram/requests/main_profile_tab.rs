//! `setMainProfileTab` (schema 1.8.68, line 15238).
use crate::ids::RequestId;
use crate::profile_tab::ProfileTab;
use serde_json::json;

/// `setMainProfileTab main_profile_tab:ProfileTab = Ok;` for the current
/// user.
pub fn set_main_profile_tab(extra: RequestId, tab: ProfileTab) -> String {
    json!({
        "@type": "setMainProfileTab",
        "@extra": extra.as_extra(),
        "main_profile_tab": tab.to_value(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::set_main_profile_tab;
    use crate::ids::RequestId;
    use crate::profile_tab::ProfileTab;

    #[test]
    fn builds_the_request() {
        let json: serde_json::Value =
            serde_json::from_str(&set_main_profile_tab(RequestId(3), ProfileTab::Music)).unwrap();
        assert_eq!(json["@type"], "setMainProfileTab");
        assert_eq!(json["main_profile_tab"]["@type"], "profileTabMusic");
    }
}

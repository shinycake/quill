use crate::ids::RequestId;
use serde_json::json;

/// `getInstalledBackgrounds` (`schema/td_api.tl:15963`). Response is
/// `backgrounds`.
pub fn get_installed_backgrounds(extra: RequestId, for_dark_theme: bool) -> String {
    json!({
        "@type": "getInstalledBackgrounds",
        "@extra": extra.as_extra(),
        "for_dark_theme": for_dark_theme,
    })
    .to_string()
}

/// `setDefaultBackground` (`schema/td_api.tl:15957`) with an installed
/// background and `type: null` (the background keeps its own type). Response
/// is `background`.
pub fn set_default_background(
    extra: RequestId,
    background_id: i64,
    for_dark_theme: bool,
) -> String {
    json!({
        "@type": "setDefaultBackground",
        "@extra": extra.as_extra(),
        "background": { "@type": "inputBackgroundRemote", "background_id": background_id },
        "type": null,
        "for_dark_theme": for_dark_theme,
    })
    .to_string()
}

/// `deleteDefaultBackground` (`schema/td_api.tl:15960`). Response is `ok`.
pub fn delete_default_background(extra: RequestId, for_dark_theme: bool) -> String {
    json!({
        "@type": "deleteDefaultBackground",
        "@extra": extra.as_extra(),
        "for_dark_theme": for_dark_theme,
    })
    .to_string()
}

/// `removeInstalledBackground` (`schema/td_api.tl:15966`). Response is `ok`.
pub fn remove_installed_background(extra: RequestId, background_id: i64) -> String {
    json!({
        "@type": "removeInstalledBackground",
        "@extra": extra.as_extra(),
        "background_id": background_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_request_shapes_match_the_schema() {
        let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
        let v = parse(get_installed_backgrounds(RequestId(1), true));
        assert_eq!(v["@type"], "getInstalledBackgrounds");
        assert_eq!(v["for_dark_theme"], true);
        let v = parse(set_default_background(RequestId(2), 99, false));
        assert_eq!(v["background"]["@type"], "inputBackgroundRemote");
        assert_eq!(v["background"]["background_id"], 99);
        assert!(v["type"].is_null());
        assert_eq!(v["for_dark_theme"], false);
        let v = parse(remove_installed_background(RequestId(3), 99));
        assert_eq!(v["background_id"], 99);
        let v = parse(delete_default_background(RequestId(4), true));
        assert_eq!(v["@type"], "deleteDefaultBackground");
    }
}

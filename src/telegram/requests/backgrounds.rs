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

/// `setDefaultBackground` with an image file from disk
/// (`inputBackgroundLocal`, `schema/td_api.tl:8808`) as a plain photo
/// wallpaper. Response is `background`.
pub fn set_default_background_local(extra: RequestId, path: &str, for_dark_theme: bool) -> String {
    json!({
        "@type": "setDefaultBackground",
        "@extra": extra.as_extra(),
        "background": {
            "@type": "inputBackgroundLocal",
            "background": { "@type": "inputFileLocal", "path": path },
        },
        "type": { "@type": "backgroundTypeWallpaper", "is_blurred": false, "is_moving": false },
        "for_dark_theme": for_dark_theme,
    })
    .to_string()
}

/// `searchBackground` (`schema/td_api.tl:15951`): a background by its link
/// name. Response is `background`.
pub fn search_background(extra: RequestId, name: &str) -> String {
    json!({
        "@type": "searchBackground",
        "@extra": extra.as_extra(),
        "name": name,
    })
    .to_string()
}

/// `setChatBackground` (`schema/td_api.tl:13867`) with an installed
/// background and its own type. `only_for_self` keeps the other user's
/// wallpaper untouched (setting it for both needs Premium).
pub fn set_chat_background(
    extra: RequestId,
    chat_id: i64,
    background_id: i64,
    dark_theme_dimming: i32,
    only_for_self: bool,
) -> String {
    json!({
        "@type": "setChatBackground",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "background": { "@type": "inputBackgroundRemote", "background_id": background_id },
        "type": null,
        "dark_theme_dimming": dark_theme_dimming.clamp(0, 100),
        "only_for_self": only_for_self,
    })
    .to_string()
}

/// `deleteChatBackground` (`schema/td_api.tl:13873`). Response is `ok`;
/// `updateChatBackground` carries the result.
pub fn delete_chat_background(extra: RequestId, chat_id: i64, restore_previous: bool) -> String {
    json!({
        "@type": "deleteChatBackground",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "restore_previous": restore_previous,
    })
    .to_string()
}

/// `setChatTheme` (`schema/td_api.tl:13881`) with an emoji theme; an empty
/// `name` is Telegram's "no theme" (`inputChatThemeEmoji` with an empty
/// name). Response is `ok`; `updateChatTheme` carries the result.
pub fn set_chat_theme(extra: RequestId, chat_id: i64, name: &str) -> String {
    json!({
        "@type": "setChatTheme",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "theme": { "@type": "inputChatThemeEmoji", "name": name },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_wallpaper_and_theme_requests_match_the_schema() {
        let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
        let v = parse(set_chat_background(RequestId(1), 7, 9, 140, true));
        assert_eq!(v["@type"], "setChatBackground");
        assert_eq!(v["background"]["@type"], "inputBackgroundRemote");
        assert_eq!(v["background"]["background_id"], 9);
        assert!(v["type"].is_null());
        assert_eq!(v["dark_theme_dimming"], 100);
        assert_eq!(v["only_for_self"], true);
        let v = parse(delete_chat_background(RequestId(2), 7, false));
        assert_eq!(v["@type"], "deleteChatBackground");
        assert_eq!(v["restore_previous"], false);
        let v = parse(set_chat_theme(RequestId(3), 7, "🏠"));
        assert_eq!(v["theme"]["@type"], "inputChatThemeEmoji");
        assert_eq!(v["theme"]["name"], "🏠");
        let v = parse(set_default_background_local(RequestId(4), "/a/b.jpg", true));
        assert_eq!(v["background"]["@type"], "inputBackgroundLocal");
        assert_eq!(v["background"]["background"]["path"], "/a/b.jpg");
        assert_eq!(v["type"]["@type"], "backgroundTypeWallpaper");
        let v = parse(search_background(RequestId(5), "abc"));
        assert_eq!(v["name"], "abc");
    }

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

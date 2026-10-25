//! Text composition style requests (TDLib 1.8.69). The compose box itself
//! sends [`super::compose_text_with_ai`] and [`super::fix_text_with_ai`].
use crate::ids::RequestId;
use serde_json::json;

/// `createTextCompositionStyle title:string custom_emoji_id:int64
/// prompt:string show_creator:Bool = TextCompositionStyle`
/// (`schema/td_api.tl:13021`).
pub fn create_text_composition_style(
    extra: RequestId,
    title: &str,
    custom_emoji_id: i64,
    prompt: &str,
    show_creator: bool,
) -> String {
    json!({
        "@type": "createTextCompositionStyle",
        "@extra": extra.as_extra(),
        "title": title,
        "custom_emoji_id": custom_emoji_id.to_string(),
        "prompt": prompt,
        "show_creator": show_creator,
    })
    .to_string()
}

/// `editTextCompositionStyle name:string title:string custom_emoji_id:int64
/// prompt:string show_creator:Bool = TextCompositionStyle` (`:13029`).
pub fn edit_text_composition_style(
    extra: RequestId,
    name: &str,
    title: &str,
    custom_emoji_id: i64,
    prompt: &str,
    show_creator: bool,
) -> String {
    json!({
        "@type": "editTextCompositionStyle",
        "@extra": extra.as_extra(),
        "name": name,
        "title": title,
        "custom_emoji_id": custom_emoji_id.to_string(),
        "prompt": prompt,
        "show_creator": show_creator,
    })
    .to_string()
}

/// `getTextCompositionStyleExample name:string example_number:int32 =
/// TextCompositionStyleExample` (`:13041`).
pub fn get_text_composition_style_example(
    extra: RequestId,
    name: &str,
    example_number: i32,
) -> String {
    json!({
        "@type": "getTextCompositionStyleExample",
        "@extra": extra.as_extra(),
        "name": name,
        "example_number": example_number,
    })
    .to_string()
}

/// The style methods that take only the style's name:
/// `deleteTextCompositionStyle` (`:13033`), `searchTextCompositionStyle`
/// (`:13036`), `addTextCompositionStyle` (`:13046`) and
/// `removeTextCompositionStyle` (`:13050`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleByName {
    Delete,
    Search,
    Add,
    Remove,
}

pub fn text_composition_style_by_name(extra: RequestId, method: StyleByName, name: &str) -> String {
    let method = match method {
        StyleByName::Delete => "deleteTextCompositionStyle",
        StyleByName::Search => "searchTextCompositionStyle",
        StyleByName::Add => "addTextCompositionStyle",
        StyleByName::Remove => "removeTextCompositionStyle",
    };
    json!({
        "@type": method,
        "@extra": extra.as_extra(),
        "name": name,
    })
    .to_string()
}

use super::*;
use serde_json::Value;

/// Bots slice: one `inlineQueryResults` page (TDLib 1.8.67,
/// `schema/td_api.tl:7716`) — the `getInlineQueryResults` answer (schema
/// line 13019).
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultsPage {
    pub inline_query_id: i64,
    pub button: Option<InlineQueryResultsButton>,
    pub results: Vec<InlineQueryResultSummary>,
    /// Slice S9: the nested `animation` objects of `inlineQueryResultAnimation`
    /// entries (schema 1.8.67, line 7658), parsed to `AnimationItem`s for the
    /// GIF panel search. The per-result summaries above deliberately drop
    /// them; collecting here keeps Loop 3's summary contract untouched.
    pub animations: Vec<AnimationItem>,
    /// Slice S9: files referenced by `animations` (same `remember_files`
    /// shape as S8's `Stickers` payload).
    pub files: Vec<ParsedFile>,
    pub next_offset: String,
}

/// Bots slice: one inline query result, reduced to the fields the picker
/// needs. Thumbnails are deliberately NOT parsed (no URL on the wire;
/// file-download wiring is out of slice).
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultSummary {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
}

/// Bots slice: `inlineQueryResultsButton` (TDLib 1.8.67,
/// `schema/td_api.tl:7708`) — the button shown above the results.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQueryResultsButton {
    pub text: String,
    pub kind: String,
    /// StartBot parameter, empty unless `kind == "start_bot"`
    /// (schema `td_api.tl:7701`).
    pub parameter: String,
    /// WebApp url, empty unless `kind == "web_app"` (schema `td_api.tl:7704`).
    pub url: String,
}

/// Bots slice: one `InlineQueryResult` (schema 1.8.67, lines 7628–7695),
/// reduced to id/kind/title/description. Variants without a title or
/// description field (contact, venue, game, audio, sticker) get empty
/// strings; an unknown future variant degrades to `kind: "unknown"`.
pub(crate) fn parse_inline_query_result(value: &Value) -> InlineQueryResultSummary {
    let type_name = value
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = match type_name {
        "inlineQueryResultArticle" => "article",
        "inlineQueryResultContact" => "contact",
        "inlineQueryResultLocation" => "location",
        "inlineQueryResultVenue" => "venue",
        "inlineQueryResultGame" => "game",
        "inlineQueryResultAnimation" => "animation",
        "inlineQueryResultAudio" => "audio",
        "inlineQueryResultDocument" => "document",
        "inlineQueryResultPhoto" => "photo",
        "inlineQueryResultSticker" => "sticker",
        "inlineQueryResultVideo" => "video",
        "inlineQueryResultVoiceNote" => "voice_note",
        _ => "unknown",
    };
    InlineQueryResultSummary {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: kind.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// Bots slice: `inlineQueryResultsButton` (schema 1.8.67, line 7708).
pub(crate) fn parse_inline_query_results_button(value: &Value) -> InlineQueryResultsButton {
    let button_type = value.get("type");
    let kind = match button_type
        .and_then(|type_value| type_value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("inlineQueryResultsButtonTypeStartBot") => "start_bot",
        Some("inlineQueryResultsButtonTypeWebApp") => "web_app",
        _ => "unknown",
    };
    InlineQueryResultsButton {
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: kind.to_string(),
        parameter: button_type
            .and_then(|type_value| type_value.get("parameter"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        url: button_type
            .and_then(|type_value| type_value.get("url"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

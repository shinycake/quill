//! Rich text (`richText*`) to HTML: escaping, the links the page may carry,
//! and the inline tags tdesktop writes for each style
//! (`EntityTagsFor` in `iv_rich_message_html_export.cpp`).
use super::blocks::Ctx;
use super::media::MediaKind;
use crate::json_access::JsonExt;
use serde_json::Value;

/// How deep blocks and rich texts may nest before the rest is dropped
/// (panic-safety rule 13: received data is walked with a limit).
pub(super) const MAX_DEPTH: usize = 48;

/// Append `value` with `<`, `>`, `&`, `"` and `'` escaped; newlines become
/// `<br>` when `line_breaks` is set.
pub(super) fn push_escaped(out: &mut String, value: &str, line_breaks: bool) {
    for ch in value.chars() {
        match ch {
            '\n' if line_breaks => out.push_str("<br>\n"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
}

pub(super) fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    push_escaped(&mut out, value, false);
    out
}

/// The URL when the page may link to it: an in-page `#fragment`, or one of
/// the schemes tdesktop allows (`SafeUrl`). `javascript:`, `data:`,
/// `file:` and the like are dropped, so message text can never run code
/// in the saved page.
pub(super) fn safe_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.starts_with('#') {
        return Some(trimmed.to_string());
    }
    const ALLOWED: [&str; 6] = ["http://", "https://", "tg://", "mailto:", "tel:", "ftp://"];
    let lower = trimmed.to_ascii_lowercase();
    ALLOWED
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        .then(|| trimmed.to_string())
}

/// ` href="…"` for a safe URL, otherwise nothing.
pub(super) fn href(url: &str) -> String {
    safe_url(url)
        .map(|url| format!(" href=\"{}\"", escaped(&url)))
        .unwrap_or_default()
}

/// `<a name="…"></a>` for a non-empty anchor name.
pub(super) fn anchor(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("<a name=\"{}\"></a>", escaped(name))
    }
}

/// A `media/…` path as an attribute value: percent-encoded (spaces,
/// `#`, `?` and non-ASCII in file names), then HTML-escaped.
pub(super) fn media_href(relative: &str) -> String {
    let mut encoded = String::with_capacity(relative.len());
    for byte in relative.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    escaped(&encoded)
}

/// tdesktop's `tg-time` format letters (`FormattedDateFormat`): `r` for
/// relative, else `w` (weekday), `D`/`d` (long/short date), `T`/`t`
/// (long/short time).
fn time_format(formatting: &Value) -> String {
    if formatting.at("@type") == "dateTimeFormattingTypeRelative" {
        return "r".into();
    }
    let precision = |field: &str| match formatting.at(field).at("@type").as_str() {
        Some("dateTimePartPrecisionLong") => 2,
        Some("dateTimePartPrecisionShort") => 1,
        _ => 0,
    };
    let mut format = String::new();
    if formatting.at("show_day_of_week") == true {
        format.push('w');
    }
    match precision("date_precision") {
        2 => format.push('D'),
        1 => format.push('d'),
        _ => {}
    }
    match precision("time_precision") {
        2 => format.push('T'),
        1 => format.push('t'),
        _ => {}
    }
    format
}

fn str_field<'v>(value: &'v Value, field: &str) -> &'v str {
    value.at(field).as_str().unwrap_or("")
}

/// Append one `RichText` node. `line_breaks` is off inside code, where
/// newlines stay as they are.
pub(super) fn rich_text(
    out: &mut String,
    value: &Value,
    ctx: &mut Ctx<'_>,
    line_breaks: bool,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let inner = value.at("text");
    let wrap = |out: &mut String, ctx: &mut Ctx<'_>, open: &str, close: &str, breaks: bool| {
        out.push_str(open);
        rich_text(out, inner, ctx, breaks, depth + 1);
        out.push_str(close);
    };
    let link = |out: &mut String, ctx: &mut Ctx<'_>, url: &str| {
        let href = href(url);
        if href.is_empty() {
            rich_text(out, inner, ctx, line_breaks, depth + 1);
        } else {
            out.push_str(&format!("<a{href}>"));
            rich_text(out, inner, ctx, line_breaks, depth + 1);
            out.push_str("</a>");
        }
    };
    match value.at("@type").as_str().unwrap_or("") {
        "richTextPlain" => push_escaped(out, str_field(value, "text"), line_breaks),
        "richTexts" => {
            if let Some(texts) = value.at("texts").as_array() {
                for text in texts {
                    rich_text(out, text, ctx, line_breaks, depth + 1);
                }
            }
        }
        "richTextBold" => wrap(out, ctx, "<strong>", "</strong>", line_breaks),
        "richTextItalic" => wrap(out, ctx, "<em>", "</em>", line_breaks),
        "richTextUnderline" => wrap(out, ctx, "<u>", "</u>", line_breaks),
        "richTextStrikethrough" => wrap(out, ctx, "<s>", "</s>", line_breaks),
        "richTextSpoiler" => wrap(out, ctx, "<tg-spoiler>", "</tg-spoiler>", line_breaks),
        "richTextSubscript" => wrap(out, ctx, "<sub>", "</sub>", line_breaks),
        "richTextSuperscript" => wrap(out, ctx, "<sup>", "</sup>", line_breaks),
        "richTextMarked" => wrap(out, ctx, "<mark>", "</mark>", line_breaks),
        "richTextFixed" => wrap(out, ctx, "<code>", "</code>", false),
        "richTextUrl" => link(out, ctx, str_field(value, "url")),
        "richTextEmailAddress" => link(
            out,
            ctx,
            &format!("mailto:{}", str_field(value, "email_address")),
        ),
        "richTextPhoneNumber" => link(
            out,
            ctx,
            &format!("tel:{}", str_field(value, "phone_number")),
        ),
        "richTextMention" => link(
            out,
            ctx,
            &format!("https://t.me/{}", str_field(value, "username")),
        ),
        "richTextMentionName" => match value.at("user_id").as_i64() {
            Some(user_id) if user_id > 0 => link(out, ctx, &format!("tg://user?id={user_id}")),
            _ => rich_text(out, inner, ctx, line_breaks, depth + 1),
        },
        "richTextReferenceLink" => link(
            out,
            ctx,
            &format!("#{}", str_field(value, "reference_name")),
        ),
        "richTextAnchorLink" => link(out, ctx, &format!("#{}", str_field(value, "anchor_name"))),
        "richTextReference" => {
            out.push_str(&anchor(str_field(value, "name")));
            rich_text(out, inner, ctx, line_breaks, depth + 1);
        }
        "richTextAnchor" => out.push_str(&anchor(str_field(value, "name"))),
        "richTextCustomEmoji" => {
            let id = value
                .at("custom_emoji_id")
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    value
                        .at("custom_emoji_id")
                        .as_i64()
                        .map(|id| id.to_string())
                })
                .filter(|id| id.bytes().all(|b| b.is_ascii_digit()) && !id.is_empty());
            let alt = escaped(str_field(value, "alternative_text"));
            match id {
                Some(id) => out.push_str(&format!("<tg-emoji emoji-id=\"{id}\">{alt}</tg-emoji>")),
                None => out.push_str(&alt),
            }
        }
        "richTextIcon" => {
            let document = value.at("document");
            if let Some(path) = ctx.media(MediaKind::Image, document, document.at("document")) {
                out.push_str(&format!(
                    "<img class=\"inline-image\" src=\"{}\" alt=\"\">",
                    media_href(&path)
                ));
            }
        }
        "richTextMathematicalExpression" => {
            out.push_str("<tg-math>");
            push_escaped(out, str_field(value, "expression"), false);
            out.push_str("</tg-math>");
        }
        "richTextButton" => rich_text(
            out,
            value.at("button").at("text"),
            ctx,
            line_breaks,
            depth + 1,
        ),
        "richTextDateTime" => {
            let formatting = value.at("formatting_type");
            match value.at("unix_time").as_i64() {
                Some(unix) if !formatting.is_null() => wrap(
                    out,
                    ctx,
                    &format!(
                        "<tg-time unix=\"{unix}\" format=\"{}\">",
                        time_format(formatting)
                    ),
                    "</tg-time>",
                    line_breaks,
                ),
                _ => rich_text(out, inner, ctx, line_breaks, depth + 1),
            }
        }
        // Hashtags, cashtags, bot commands, bank cards, TON addresses and
        // diffs keep their text without a tag, as in tdesktop.
        _ => rich_text(out, inner, ctx, line_breaks, depth + 1),
    }
}

/// The rich text as HTML, or `None` when it renders to nothing.
pub(super) fn rich_text_html(
    value: &Value,
    ctx: &mut Ctx<'_>,
    line_breaks: bool,
) -> Option<String> {
    let mut out = String::new();
    rich_text(&mut out, value, ctx, line_breaks, 0);
    (!out.is_empty()).then_some(out)
}

/// The plain text of a rich text (for the page title).
pub(super) fn plain_text(value: &Value, out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match value.at("@type").as_str().unwrap_or("") {
        "richTextPlain" => out.push_str(str_field(value, "text")),
        "richTexts" => {
            if let Some(texts) = value.at("texts").as_array() {
                for text in texts {
                    plain_text(text, out, depth + 1);
                }
            }
        }
        "richTextCustomEmoji" => out.push_str(str_field(value, "alternative_text")),
        "richTextMathematicalExpression" => out.push_str(str_field(value, "expression")),
        "richTextButton" => plain_text(value.at("button").at("text"), out, depth + 1),
        _ => plain_text(value.at("text"), out, depth + 1),
    }
}

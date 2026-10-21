//! The whole saved page: the document around the blocks (tdesktop
//! `GenerateHtml`) and its title (`TitleFromPage`).
use super::blocks::{Ctx, blocks};
use super::media::{MAX_NAME_CHARS, MediaSource, Saved};
use super::style::{SCRIPT, STYLE};
use super::text::{MAX_DEPTH, escaped, plain_text};
use crate::json_access::JsonExt;
use crate::lang::{keys, tr};
use serde_json::Value;
use std::collections::HashMap;

/// Names the saved page so it can be told apart from other HTML (tdesktop
/// writes its own `RichExportGeneratorMarker`).
pub const GENERATOR: &str = "Quill rich message export";

/// Text the page itself shows, in the interface language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLabels {
    pub media_missing: String,
    pub embed: String,
}

impl PageLabels {
    pub fn current() -> Self {
        Self {
            media_missing: tr(keys::EXPORT_HTML_MEDIA_MISSING).to_string(),
            embed: tr(keys::EXPORT_HTML_EMBED).to_string(),
        }
    }
}

/// Render `rich` (a TDLib `richMessage`) through `media`.
pub(crate) fn render(
    rich: &Value,
    title: &str,
    labels: &PageLabels,
    date: &dyn Fn(i64) -> String,
    media: &mut dyn MediaSource,
) -> String {
    let mut ctx = Ctx {
        media,
        labels,
        date,
        slideshow_used: false,
    };
    let mut body = String::new();
    blocks(&mut body, rich.at("blocks"), &mut ctx, 0);
    let dir = if rich.at("is_rtl") == true {
        " dir=\"rtl\""
    } else {
        ""
    };
    let mut html = format!(
        "<!DOCTYPE html>\n<html{dir}>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"generator\" content=\"{GENERATOR}\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n<article>\n",
        escaped(title)
    );
    html.push_str(&body);
    html.push_str("</article>\n");
    if ctx.slideshow_used {
        html.push_str("<script>\n");
        html.push_str(SCRIPT);
        html.push_str("</script>\n");
    }
    html.push_str("</body>\n</html>\n");
    html
}

/// The page for `rich` with the files in `media` (TDLib file id to its
/// `media/…` path); files not in it show as "Media unavailable".
pub fn page_html(
    rich: &Value,
    title: &str,
    labels: &PageLabels,
    date: &dyn Fn(i64) -> String,
    media: HashMap<i32, String>,
) -> String {
    render(rich, title, labels, date, &mut Saved(media))
}

fn first_text(list: &Value, headings: bool, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    for block in list.as_array()? {
        let field = match (block.at("@type").as_str().unwrap_or(""), headings) {
            ("pageBlockTitle", true) => "title",
            ("pageBlockSubtitle", true) => "subtitle",
            ("pageBlockHeader", true) => "header",
            ("pageBlockSubheader", true) => "subheader",
            ("pageBlockKicker", true) => "kicker",
            ("pageBlockSectionHeading", true) | ("pageBlockParagraph", false) => "text",
            ("pageBlockCover", _) => {
                let cover = serde_json::json!([block.at("cover")]);
                if let Some(text) = first_text(&cover, headings, depth + 1) {
                    return Some(text);
                }
                continue;
            }
            _ => continue,
        };
        let mut text = String::new();
        plain_text(block.at(field), &mut text, 0);
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    None
}

/// tdesktop `TitleFromPage`: the first heading, else the start of the
/// first paragraph.
pub fn title_from_page(rich: &Value) -> Option<String> {
    let blocks = rich.at("blocks");
    first_text(blocks, true, 0).or_else(|| {
        first_text(blocks, false, 0).map(|text| {
            let head: String = text.chars().take(MAX_NAME_CHARS).collect();
            head.trim().to_string()
        })
    })
}

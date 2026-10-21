//! Page blocks (`pageBlock*`) to HTML, block for block what tdesktop's
//! `SerializeBlock` writes for the same content. Files and button rows
//! are left out, as tdesktop does.
use super::media::{MediaKind, MediaSource, best_photo_size};
use super::page::PageLabels;
use super::text::{MAX_DEPTH, anchor, escaped, href, media_href, rich_text, rich_text_html};
use crate::album::{ALBUM_MAX_WIDTH, ALBUM_MIN_WIDTH, ALBUM_SPACING, layout_media_group};
use crate::json_access::JsonExt;
use serde_json::Value;

/// What a render needs besides the blocks.
pub(crate) struct Ctx<'a> {
    pub media: &'a mut dyn MediaSource,
    pub labels: &'a PageLabels,
    /// A unix time as the page shows it (`FormatExportDate`).
    pub date: &'a dyn Fn(i64) -> String,
    /// A slideshow with controls was written, so the page needs its script.
    pub slideshow_used: bool,
}

impl Ctx<'_> {
    pub(super) fn media(&mut self, kind: MediaKind, meta: &Value, file: &Value) -> Option<String> {
        self.media.path(kind, meta, file)
    }

    fn date_label(&self, unix: Option<i64>) -> Option<String> {
        unix.filter(|unix| *unix > 0).map(|unix| (self.date)(unix))
    }
}

/// One photo or video, as a single block or an item of a collage or
/// slideshow.
struct MediaItem<'v> {
    kind: MediaKind,
    meta: &'v Value,
    file: &'v Value,
    width: i64,
    height: i64,
    autoplay: bool,
    looped: bool,
    spoiler: bool,
}

impl<'v> MediaItem<'v> {
    /// The photo, video or animation a block shows.
    fn of(block: &'v Value) -> Option<Self> {
        let size = |meta: &Value| {
            (
                meta.at("width").as_i64().unwrap_or(0),
                meta.at("height").as_i64().unwrap_or(0),
            )
        };
        let spoiler = block.at("has_spoiler") == true;
        match block.at("@type").as_str().unwrap_or("") {
            "pageBlockPhoto" => {
                let photo = block.at("photo");
                let (file, width, height) = best_photo_size(photo).unwrap_or((&Value::Null, 0, 0));
                Some(Self {
                    kind: MediaKind::Photo,
                    meta: photo,
                    file,
                    width,
                    height,
                    autoplay: false,
                    looped: false,
                    spoiler,
                })
            }
            "pageBlockVideo" => {
                let video = block.at("video");
                let (width, height) = size(video);
                Some(Self {
                    kind: MediaKind::Video,
                    meta: video,
                    file: video.at("video"),
                    width,
                    height,
                    autoplay: block.at("need_autoplay") == true,
                    looped: block.at("is_looped") == true,
                    spoiler,
                })
            }
            "pageBlockAnimation" => {
                let animation = block.at("animation");
                let (width, height) = size(animation);
                Some(Self {
                    kind: MediaKind::Video,
                    meta: animation,
                    file: animation.at("animation"),
                    width,
                    height,
                    autoplay: block.at("need_autoplay") == true,
                    looped: true,
                    spoiler,
                })
            }
            _ => None,
        }
    }
}

fn missing(out: &mut String, label: &str) {
    out.push_str(&format!(
        "<div class=\"media-missing\">{}</div>",
        escaped(label)
    ));
}

/// tdesktop `SerializeMediaItem`: `<img>` or `<video>`, or "Media
/// unavailable" when the file could not be saved.
fn media_item(out: &mut String, item: &MediaItem<'_>, ctx: &mut Ctx<'_>) {
    let dimensions = if item.width > 0 && item.height > 0 {
        format!(" width=\"{}\" height=\"{}\"", item.width, item.height)
    } else {
        String::new()
    };
    let hidden = if item.spoiler { " tg-spoiler" } else { "" };
    let Some(path) = ctx.media(item.kind, item.meta, item.file) else {
        let label = ctx.labels.media_missing.clone();
        missing(out, &label);
        return;
    };
    let src = media_href(&path);
    if item.kind == MediaKind::Photo {
        out.push_str(&format!(
            "<img src=\"{src}\" loading=\"lazy\"{dimensions}{hidden}>"
        ));
        return;
    }
    let flags = if item.autoplay {
        " autoplay muted loop playsinline"
    } else if item.looped {
        " controls loop preload=\"metadata\""
    } else {
        " controls preload=\"metadata\""
    };
    out.push_str(&format!(
        "<video src=\"{src}\"{flags}{dimensions}{hidden}></video>"
    ));
}

/// `<figcaption>` with the caption text, then its credit on a new line
/// (tdesktop `ParseCaption` joins them with a line break).
fn caption(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    let text = rich_text_html(value.at("text"), ctx, true);
    let credit = rich_text_html(value.at("credit"), ctx, true);
    let joined = match (text, credit) {
        (Some(text), Some(credit)) => format!("{text}<br>\n{credit}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => return,
    };
    out.push_str(&format!("<figcaption>{joined}</figcaption>"));
}

/// `<cite>` with a quote's credit.
fn credit(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    if let Some(credit) = rich_text_html(value, ctx, true) {
        out.push_str(&format!("<cite>{credit}</cite>"));
    }
}

/// `<tag>` + rich text + `</tag>`.
fn text_block(out: &mut String, open: &str, close: &str, text: &Value, ctx: &mut Ctx<'_>) {
    out.push_str(open);
    rich_text(out, text, ctx, true, 0);
    out.push_str(close);
    out.push('\n');
}

pub(super) fn blocks(out: &mut String, list: &Value, ctx: &mut Ctx<'_>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(list) = list.as_array() {
        for value in list {
            block(out, value, ctx, depth);
        }
    }
}

fn heading(out: &mut String, level: i64, text: &Value, ctx: &mut Ctx<'_>) {
    let level = level.clamp(1, 6);
    text_block(
        out,
        &format!("<h{level}>"),
        &format!("</h{level}>"),
        text,
        ctx,
    );
}

fn block(out: &mut String, value: &Value, ctx: &mut Ctx<'_>, depth: usize) {
    match value.at("@type").as_str().unwrap_or("") {
        "pageBlockTitle" => heading(out, 1, value.at("title"), ctx),
        "pageBlockSubtitle" => heading(out, 2, value.at("subtitle"), ctx),
        "pageBlockHeader" => heading(out, 3, value.at("header"), ctx),
        "pageBlockSubheader" => heading(out, 4, value.at("subheader"), ctx),
        "pageBlockKicker" => heading(out, 5, value.at("kicker"), ctx),
        "pageBlockSectionHeading" => heading(
            out,
            value.at("size").as_i64().unwrap_or(2),
            value.at("text"),
            ctx,
        ),
        "pageBlockParagraph" => text_block(out, "<p>", "</p>", value.at("text"), ctx),
        "pageBlockFooter" => text_block(out, "<footer>", "</footer>", value.at("footer"), ctx),
        "pageBlockThinking" => text_block(
            out,
            "<blockquote class=\"thinking\">",
            "</blockquote>",
            value.at("text"),
            ctx,
        ),
        "pageBlockAuthorDate" => {
            out.push_str("<p class=\"byline\">");
            let author = rich_text_html(value.at("author"), ctx, true);
            let date = ctx.date_label(value.at("publish_date").as_i64());
            if let Some(author) = &author {
                out.push_str(author);
            }
            if let Some(date) = date {
                if author.is_some() {
                    out.push_str(" · ");
                }
                out.push_str(&escaped(&date));
            }
            out.push_str("</p>\n");
        }
        "pageBlockPreformatted" => {
            let language = value.at("language").as_str().unwrap_or("").trim();
            let class = if language.is_empty() {
                String::new()
            } else {
                format!(" class=\"language-{}\"", escaped(language))
            };
            out.push_str(&format!("<pre class=\"code\"><code{class}>"));
            rich_text(out, value.at("text"), ctx, false, 0);
            out.push_str("</code></pre>\n");
        }
        "pageBlockDivider" => out.push_str("<hr>\n"),
        "pageBlockAnchor" => {
            out.push_str(&anchor(value.at("name").as_str().unwrap_or("")));
            out.push('\n');
        }
        "pageBlockMathematicalExpression" => {
            // Without a formula renderer the source goes in as tdesktop's
            // fallback writes it.
            out.push_str(&format!(
                "<tg-math-block>{}</tg-math-block>\n",
                escaped(value.at("expression").as_str().unwrap_or(""))
            ));
        }
        "pageBlockList" => list(out, value, ctx, depth),
        "pageBlockBlockQuote" => {
            out.push_str("<blockquote>");
            blocks(out, value.at("blocks"), ctx, depth + 1);
            credit(out, value.at("credit"), ctx);
            out.push_str("</blockquote>\n");
        }
        "pageBlockExpandableBlockQuote" | "pageBlockPullQuote" => {
            let tag = if value.at("@type") == "pageBlockPullQuote" {
                "aside"
            } else {
                "blockquote"
            };
            out.push_str(&format!("<{tag}>"));
            rich_text(out, value.at("text"), ctx, true, 0);
            credit(out, value.at("credit"), ctx);
            out.push_str(&format!("</{tag}>\n"));
        }
        "pageBlockPhoto" | "pageBlockVideo" | "pageBlockAnimation" => {
            let Some(item) = MediaItem::of(value) else {
                return;
            };
            out.push_str("<figure class=\"media\">");
            let link = if item.kind == MediaKind::Photo {
                href(value.at("url").as_str().unwrap_or(""))
            } else {
                String::new()
            };
            if !link.is_empty() {
                out.push_str(&format!("<a{link}>"));
            }
            media_item(out, &item, ctx);
            if !link.is_empty() {
                out.push_str("</a>");
            }
            caption(out, value.at("caption"), ctx);
            out.push_str("</figure>\n");
        }
        "pageBlockAudio" | "pageBlockVoiceNote" => audio(out, value, ctx),
        "pageBlockCover" => {
            if depth < MAX_DEPTH {
                block(out, value.at("cover"), ctx, depth + 1);
            }
        }
        "pageBlockEmbedded" => {
            out.push_str("<figure class=\"embed\">");
            let url = value.at("url").as_str().unwrap_or("");
            let link = href(url);
            if link.is_empty() {
                let label = ctx.labels.embed.clone();
                missing(out, &label);
            } else {
                out.push_str(&format!("<a{link}>{}</a>", escaped(url)));
            }
            caption(out, value.at("caption"), ctx);
            out.push_str("</figure>\n");
        }
        "pageBlockEmbeddedPost" => embedded_post(out, value, ctx, depth),
        "pageBlockCollage" => collage(out, value, ctx),
        "pageBlockSlideshow" => slideshow(out, value, ctx),
        "pageBlockChatLink" => {
            let title = escaped(value.at("title").as_str().unwrap_or(""));
            let username = value.at("username").as_str().unwrap_or("");
            let link = if username.is_empty() {
                String::new()
            } else {
                href(&format!("https://t.me/{username}"))
            };
            if link.is_empty() {
                out.push_str(&format!("<div class=\"channel\">{title}</div>\n"));
            } else {
                out.push_str(&format!(
                    "<div class=\"channel\"><a{link}>{title}</a></div>\n"
                ));
            }
        }
        "pageBlockTable" => table(out, value, ctx),
        "pageBlockDetails" => {
            let open = if value.at("is_open") == true {
                " open"
            } else {
                ""
            };
            out.push_str(&format!("<details{open}><summary>"));
            rich_text(out, value.at("header"), ctx, true, 0);
            out.push_str("</summary>\n");
            blocks(out, value.at("blocks"), ctx, depth + 1);
            out.push_str("</details>\n");
        }
        "pageBlockRelatedArticles" => related(out, value, ctx),
        "pageBlockMap" => map(out, value, ctx),
        // Documents, button rows and unsupported blocks: nothing.
        _ => {}
    }
}

/// `<ul>`, or `<ol>` when the items are numbered (TDLib gives each item a
/// numbering `type`; an empty one is a bullet list). An item's first
/// paragraph is its text, the rest of its blocks follow it.
fn list(out: &mut String, value: &Value, ctx: &mut Ctx<'_>, depth: usize) {
    let Some(items) = value.at("items").as_array() else {
        return;
    };
    let kind = items
        .first()
        .and_then(|item| item.at("type").as_str())
        .unwrap_or("");
    let ordered = !kind.is_empty();
    if ordered {
        let known = ["1", "a", "A", "i", "I"].contains(&kind);
        let type_attr = if known {
            format!(" type=\"{kind}\"")
        } else {
            String::new()
        };
        out.push_str(&format!("<ol{type_attr}>\n"));
    } else {
        out.push_str("<ul>\n");
    }
    for item in items {
        out.push_str("<li");
        if let Some(number) = item.at("value").as_i64().filter(|n| ordered && *n > 0) {
            out.push_str(&format!(" value=\"{number}\""));
        }
        out.push('>');
        if item.at("has_checkbox") == true {
            let checked = if item.at("is_checked") == true {
                " checked"
            } else {
                ""
            };
            out.push_str(&format!("<input type=\"checkbox\" disabled{checked}> "));
        }
        let children = item
            .at("blocks")
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let rest = match children.split_first() {
            Some((first, rest)) if first.at("@type") == "pageBlockParagraph" => {
                rich_text(out, first.at("text"), ctx, true, 0);
                rest
            }
            _ => children,
        };
        if depth < MAX_DEPTH {
            for child in rest {
                block(out, child, ctx, depth + 1);
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
}

fn audio(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    let voice = value.at("@type") == "pageBlockVoiceNote";
    let meta = if voice {
        value.at("voice_note")
    } else {
        value.at("audio")
    };
    let file = if voice {
        meta.at("voice")
    } else {
        meta.at("audio")
    };
    out.push_str("<figure class=\"audio\">");
    match ctx.media(MediaKind::Audio, meta, file) {
        Some(path) => out.push_str(&format!(
            "<audio controls src=\"{}\"></audio>",
            media_href(&path)
        )),
        None => {
            let name = meta.at("file_name").as_str().unwrap_or("");
            let label = if name.is_empty() {
                ctx.labels.media_missing.clone()
            } else {
                name.to_string()
            };
            missing(out, &label);
        }
    }
    let parts: Vec<String> = ["performer", "title"]
        .iter()
        .filter_map(|field| meta.at(field).as_str().filter(|s| !s.is_empty()))
        .map(escaped)
        .collect();
    if !parts.is_empty() {
        out.push_str(&format!(
            "<div class=\"audio-meta\">{}</div>",
            parts.join(" — ")
        ));
    }
    caption(out, value.at("caption"), ctx);
    out.push_str("</figure>\n");
}

fn embedded_post(out: &mut String, value: &Value, ctx: &mut Ctx<'_>, depth: usize) {
    out.push_str("<blockquote class=\"embed-post\">");
    let photo = value.at("author_photo");
    if let Some((file, _, _)) = best_photo_size(photo)
        && let Some(path) = ctx.media(MediaKind::Photo, photo, file)
    {
        out.push_str(&format!(
            "<img class=\"embed-post-photo\" src=\"{}\">",
            media_href(&path)
        ));
    }
    let mut header = Vec::new();
    let author = value.at("author").as_str().unwrap_or("");
    if !author.is_empty() {
        header.push(escaped(author));
    }
    if let Some(date) = ctx.date_label(value.at("date").as_i64()) {
        header.push(escaped(&date));
    }
    if !header.is_empty() {
        let joined = header.join(" · ");
        let link = href(value.at("url").as_str().unwrap_or(""));
        let inner = if link.is_empty() {
            joined
        } else {
            format!("<a{link}>{joined}</a>")
        };
        out.push_str(&format!("<div class=\"embed-post-header\">{inner}</div>"));
    }
    blocks(out, value.at("blocks"), ctx, depth + 1);
    caption(out, value.at("caption"), ctx);
    out.push_str("</blockquote>\n");
}

fn media_items(value: &Value) -> Vec<MediaItem<'_>> {
    value
        .at("blocks")
        .as_array()
        .map(|items| items.iter().filter_map(MediaItem::of).collect())
        .unwrap_or_default()
}

/// The most tiles one mosaic lays out: an album's limit. The album layout
/// gives nothing past 12, so a bigger collage is split into several boxes
/// instead of vanishing.
const COLLAGE_TILES: usize = 10;

/// tdesktop `SerializeCollage`: the album mosaic laid out as in a chat
/// bubble, its tiles placed by percentages so it scales with the page.
fn collage(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    let items = media_items(value);
    let mut boxes = String::new();
    for tiles in items.chunks(COLLAGE_TILES) {
        collage_box(&mut boxes, tiles, ctx);
    }
    if boxes.is_empty() {
        return;
    }
    out.push_str("<tg-collage>");
    out.push_str(&boxes);
    caption(out, value.at("caption"), ctx);
    out.push_str("</tg-collage>\n");
}

/// One mosaic of up to [`COLLAGE_TILES`] tiles.
fn collage_box(out: &mut String, items: &[MediaItem<'_>], ctx: &mut Ctx<'_>) {
    let sizes: Vec<(i32, i32)> = items
        .iter()
        .map(|item| {
            let clamp = |v: i64| i32::try_from(v.max(1)).unwrap_or(i32::MAX);
            (clamp(item.width), clamp(item.height))
        })
        .collect();
    let layout = layout_media_group(&sizes, ALBUM_MAX_WIDTH, ALBUM_MIN_WIDTH, ALBUM_SPACING);
    let left = layout.iter().map(|r| r.x).min().unwrap_or(0);
    let top = layout.iter().map(|r| r.y).min().unwrap_or(0);
    let right = layout
        .iter()
        .map(|r| r.x.saturating_add(r.width))
        .max()
        .unwrap_or(0);
    let bottom = layout
        .iter()
        .map(|r| r.y.saturating_add(r.height))
        .max()
        .unwrap_or(0);
    let (width, height) = (right.saturating_sub(left), bottom.saturating_sub(top));
    if layout.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    let percent =
        |part: i32, total: i32| format!("{:.3}", f64::from(part) * 100. / f64::from(total));
    out.push_str(&format!(
        "<div class=\"collage-box\" style=\"aspect-ratio:{width}/{height}\">"
    ));
    for (rect, item) in layout.iter().zip(items) {
        out.push_str(&format!(
            "<div class=\"collage-item\" style=\"left:{}%;top:{}%;width:{}%;height:{}%\">",
            percent(rect.x.saturating_sub(left), width),
            percent(rect.y.saturating_sub(top), height),
            percent(rect.width, width),
            percent(rect.height, height),
        ));
        media_item(out, item, ctx);
        out.push_str("</div>");
    }
    out.push_str("</div>");
}

/// tdesktop `SerializeSlideshow`: one slide per item in a box shaped like
/// the widest one, with arrows and dots when there is more than one.
fn slideshow(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    let items = media_items(value);
    if items.is_empty() {
        return;
    }
    let (mut best_w, mut best_h, mut best_ratio) = (1, 1, 0.);
    for item in &items {
        let (w, h) = (item.width.max(1), item.height.max(1));
        let ratio = w as f64 / h as f64;
        if ratio > best_ratio {
            (best_w, best_h, best_ratio) = (w, h, ratio);
        }
    }
    let controls = items.len() > 1;
    if controls {
        ctx.slideshow_used = true;
    }
    out.push_str(&format!(
        "<tg-slideshow><div class=\"slideshow-box\" style=\"aspect-ratio:{best_w}/{best_h}\"><div class=\"slideshow-track\">"
    ));
    for item in &items {
        out.push_str("<div class=\"slide\">");
        media_item(out, item, ctx);
        out.push_str("</div>");
    }
    out.push_str("</div>");
    if controls {
        out.push_str("<button class=\"slideshow-prev\" type=\"button\">&#8592;</button>");
        out.push_str("<button class=\"slideshow-next\" type=\"button\">&#8594;</button>");
        out.push_str("<div class=\"slideshow-dots\">");
        for _ in &items {
            out.push_str("<button class=\"slideshow-dot\" type=\"button\"></button>");
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    caption(out, value.at("caption"), ctx);
    out.push_str("</tg-slideshow>\n");
}

fn table(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    out.push_str("<figure class=\"table-wrap\">");
    if let Some(title) = rich_text_html(value.at("caption"), ctx, true) {
        out.push_str(&format!("<div class=\"table-title\">{title}</div>"));
    }
    let classes: Vec<&str> = [
        ("is_bordered", "bordered"),
        ("is_striped", "striped"),
        ("is_compact", "compact"),
    ]
    .iter()
    .filter(|(field, _)| value.at(field) == true)
    .map(|(_, class)| *class)
    .collect();
    if classes.is_empty() {
        out.push_str("<table>\n");
    } else {
        out.push_str(&format!("<table class=\"{}\">\n", classes.join(" ")));
    }
    for row in value
        .at("cells")
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        out.push_str("<tr>");
        for cell in row.as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let tag = if cell.at("is_header") == true {
                "th"
            } else {
                "td"
            };
            let mut attributes = String::new();
            for field in ["colspan", "rowspan"] {
                if let Some(span) = cell.at(field).as_i64().filter(|n| *n > 1) {
                    attributes.push_str(&format!(" {field}=\"{span}\""));
                }
            }
            let mut style = String::new();
            match cell.at("align").at("@type").as_str() {
                Some("pageBlockHorizontalAlignmentCenter") => style.push_str("text-align:center;"),
                Some("pageBlockHorizontalAlignmentRight") => style.push_str("text-align:right;"),
                _ => {}
            }
            match cell.at("valign").at("@type").as_str() {
                Some("pageBlockVerticalAlignmentMiddle") => {
                    style.push_str("vertical-align:middle;")
                }
                Some("pageBlockVerticalAlignmentBottom") => {
                    style.push_str("vertical-align:bottom;")
                }
                _ => {}
            }
            if !style.is_empty() {
                attributes.push_str(&format!(" style=\"{style}\""));
            }
            out.push_str(&format!("<{tag}{attributes}>"));
            rich_text(out, cell.at("text"), ctx, true, 0);
            out.push_str(&format!("</{tag}>"));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table></figure>\n");
}

fn related(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    out.push_str("<section class=\"related\">");
    if let Some(title) = rich_text_html(value.at("header"), ctx, true) {
        out.push_str(&format!("<div class=\"related-title\">{title}</div>"));
    }
    for article in value
        .at("articles")
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let link = href(article.at("url").as_str().unwrap_or(""));
        if link.is_empty() {
            out.push_str("<div class=\"related-card\">");
        } else {
            out.push_str(&format!("<a class=\"related-card\"{link}>"));
        }
        let photo = article.at("photo");
        if let Some((file, _, _)) = best_photo_size(photo)
            && let Some(path) = ctx.media(MediaKind::Photo, photo, file)
        {
            out.push_str(&format!("<img src=\"{}\">", media_href(&path)));
        }
        for (field, class) in [("title", "related-name"), ("description", "related-desc")] {
            let text = article.at(field).as_str().unwrap_or("");
            if !text.is_empty() {
                out.push_str(&format!("<div class=\"{class}\">{}</div>", escaped(text)));
            }
        }
        let mut meta = Vec::new();
        let author = article.at("author").as_str().unwrap_or("");
        if !author.is_empty() {
            meta.push(escaped(author));
        }
        if let Some(date) = ctx.date_label(article.at("publish_date").as_i64()) {
            meta.push(escaped(&date));
        }
        if !meta.is_empty() {
            out.push_str(&format!(
                "<div class=\"related-meta\">{}</div>",
                meta.join(" · ")
            ));
        }
        out.push_str(if link.is_empty() {
            "</div>\n"
        } else {
            "</a>\n"
        });
    }
    out.push_str("</section>\n");
}

fn map(out: &mut String, value: &Value, ctx: &mut Ctx<'_>) {
    let location = value.at("location");
    let latitude = location.at("latitude").as_f64().unwrap_or(0.);
    let longitude = location.at("longitude").as_f64().unwrap_or(0.);
    let zoom = value.at("zoom").as_i64().filter(|z| *z > 0).unwrap_or(15);
    out.push_str("<figure class=\"map\">");
    out.push_str(&format!(
        "<tg-map lat=\"{latitude}\" long=\"{longitude}\" zoom=\"{zoom}\"></tg-map>"
    ));
    let url = format!(
        "https://www.openstreetmap.org/?mlat={latitude}&mlon={longitude}#map={zoom}/{latitude}/{longitude}"
    );
    out.push_str(&format!("<a{}>{latitude}, {longitude}</a>", href(&url)));
    caption(out, value.at("caption"), ctx);
    out.push_str("</figure>\n");
}

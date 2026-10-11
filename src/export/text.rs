//! Message text to HTML: escaping and formatting entities.
use super::model::{Entity, EntityKind};
use std::collections::BTreeSet;

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Only web, mail and Telegram links become `href`s. Anything else
/// (`javascript:`, `data:`, `file:`) is shown as text, never linked.
pub fn safe_href(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "tg://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| url.trim().to_string())
}

fn tags(kind: &EntityKind, covered: &str) -> Option<(String, &'static str)> {
    Some(match kind {
        EntityKind::Bold => ("<strong>".into(), "</strong>"),
        EntityKind::Italic => ("<em>".into(), "</em>"),
        EntityKind::Underline => ("<u>".into(), "</u>"),
        EntityKind::Strike => ("<s>".into(), "</s>"),
        EntityKind::Spoiler => ("<span class=\"spoiler\">".into(), "</span>"),
        EntityKind::Code => ("<code>".into(), "</code>"),
        EntityKind::Pre => ("<pre>".into(), "</pre>"),
        EntityKind::Quote => ("<blockquote>".into(), "</blockquote>"),
        EntityKind::Url => {
            let head = covered.split('/').next().unwrap_or_default();
            let href = if covered.contains("://") {
                covered.to_string()
            } else if head.contains(':') {
                // "javascript:x", "mailto:a@b": a scheme without `//`.
                covered.to_string()
            } else {
                format!("http://{covered}")
            };
            let href = safe_href(&href)?;
            (format!("<a href=\"{}\">", escape(&href)), "</a>")
        }
        EntityKind::Email => (
            format!("<a href=\"mailto:{}\">", escape(covered.trim())),
            "</a>",
        ),
        EntityKind::Link(url) => {
            let href = safe_href(url)?;
            (format!("<a href=\"{}\">", escape(&href)), "</a>")
        }
    })
}

/// Render `text` with `entities` (UTF-16 offsets) as HTML. The text is
/// cut at every entity edge and each piece is wrapped in the tags of the
/// entities covering it, so overlapping entities never produce misnested
/// markup.
pub fn render(text: &str, entities: &[Entity]) -> String {
    let chars: Vec<char> = text.chars().collect();
    // UTF-16 offset of each char boundary, plus the end.
    let mut utf16_at = Vec::with_capacity(chars.len() + 1);
    let mut units = 0;
    for c in &chars {
        utf16_at.push(units);
        units += c.len_utf16();
    }
    utf16_at.push(units);
    let char_index = |offset: usize| match utf16_at.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let spans: Vec<(usize, usize, &Entity)> = entities
        .iter()
        .filter(|e| e.length > 0)
        .map(|e| {
            (
                char_index(e.offset.min(units)),
                char_index((e.offset + e.length).min(units)),
                e,
            )
        })
        .filter(|(start, end, _)| start < end)
        .collect();
    let mut cuts: BTreeSet<usize> = [0, chars.len()].into();
    for (start, end, _) in &spans {
        cuts.insert(*start);
        cuts.insert(*end);
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();
    let mut out = String::with_capacity(text.len() + 16);
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let piece: String = chars[from..to].iter().collect();
        let mut covering: Vec<&(usize, usize, &Entity)> = spans
            .iter()
            .filter(|(start, end, _)| *start <= from && to <= *end)
            .collect();
        // Outer entities first: earlier start, then longer.
        covering.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
        let mut closing = Vec::new();
        let mut in_pre = false;
        for (start, end, entity) in covering {
            let covered: String = chars[*start..*end].iter().collect();
            if let Some((open, close)) = tags(&entity.kind, &covered) {
                out.push_str(&open);
                closing.push(close);
                in_pre |= matches!(entity.kind, EntityKind::Pre);
            }
        }
        let escaped = escape(&piece);
        if in_pre {
            out.push_str(&escaped);
        } else {
            out.push_str(&escaped.replace('\n', "<br>"));
        }
        for close in closing.into_iter().rev() {
            out.push_str(close);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(offset: usize, length: usize, kind: EntityKind) -> Entity {
        Entity {
            offset,
            length,
            kind,
        }
    }

    #[test]
    fn escapes_markup_and_quotes() {
        assert_eq!(
            escape("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn plain_text_keeps_line_breaks() {
        assert_eq!(render("a\nb <i>", &[]), "a<br>b &lt;i&gt;");
    }

    #[test]
    fn entities_wrap_their_range() {
        let html = render("hello world", &[entity(6, 5, EntityKind::Bold)]);
        assert_eq!(html, "hello <strong>world</strong>");
    }

    #[test]
    fn offsets_count_utf16_units() {
        // The emoji is two UTF-16 units, so "bold" starts at unit 3.
        let html = render("😀 bold", &[entity(3, 4, EntityKind::Bold)]);
        assert_eq!(html, "😀 <strong>bold</strong>");
    }

    #[test]
    fn overlapping_entities_stay_well_nested() {
        let html = render(
            "abcd",
            &[
                entity(0, 3, EntityKind::Bold),
                entity(2, 2, EntityKind::Italic),
            ],
        );
        assert_eq!(
            html,
            "<strong>ab</strong><strong><em>c</em></strong><em>d</em>"
        );
    }

    #[test]
    fn links_allow_only_known_schemes() {
        let good = render(
            "site",
            &[entity(
                0,
                4,
                EntityKind::Link("https://example.com/?a=1&b=2".into()),
            )],
        );
        assert_eq!(
            good,
            "<a href=\"https://example.com/?a=1&amp;b=2\">site</a>"
        );
        let bad = render(
            "site",
            &[entity(0, 4, EntityKind::Link("javascript:alert(1)".into()))],
        );
        assert_eq!(bad, "site");
        let bare = render("example.com", &[entity(0, 11, EntityKind::Url)]);
        assert_eq!(bare, "<a href=\"http://example.com\">example.com</a>");
        let script = render("javascript:x", &[entity(0, 12, EntityKind::Url)]);
        assert!(!script.contains("href"));
    }

    #[test]
    fn pre_keeps_raw_newlines() {
        let html = render("a\nb", &[entity(0, 3, EntityKind::Pre)]);
        assert_eq!(html, "<pre>a\nb</pre>");
    }

    #[test]
    fn out_of_range_entities_are_clamped_or_dropped() {
        let html = render(
            "ab",
            &[
                entity(1, 50, EntityKind::Bold),
                entity(9, 3, EntityKind::Italic),
            ],
        );
        assert_eq!(html, "a<strong>b</strong>");
    }
}

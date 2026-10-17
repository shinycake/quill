//! Links in a group or channel description. TDLib sends the description as
//! a plain string; tdesktop's `AboutWithEntities` parses it for links,
//! mentions and hashtags, and strips external links from groups
//! (`StripExternalLinks`). Here TDLib's own `getTextEntities` does the
//! parsing (it is callable synchronously, outside any client), and
//! [`strip_external_links`] applies the group rule.
#![deny(clippy::indexing_slicing)]

use super::{TextEntity, TextEntityKind};
use crate::link_policy::host_of;

/// The entities TDLib finds in `text`: links, emails, mentions and
/// hashtags (tdesktop's about flags). Empty without a loaded `libtdjson`
/// (offline demos and tests carry their own entities).
pub fn about_entities(text: &str) -> Vec<TextEntity> {
    if text.is_empty() {
        return Vec::new();
    }
    let Some(api) = crate::telegram::hub::TdHub::shared_api() else {
        return Vec::new();
    };
    let request = serde_json::json!({ "@type": "getTextEntities", "text": text }).to_string();
    match api.execute(&request) {
        Ok(Some(body)) => entities_from_response(text, &body),
        _ => Vec::new(),
    }
}

/// The kept entities of a `textEntities` answer over `text`, with UTF-8
/// offsets.
pub fn entities_from_response(text: &str, body: &str) -> Vec<TextEntity> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    if value.get("@type").and_then(serde_json::Value::as_str) != Some("textEntities") {
        return Vec::new();
    }
    crate::telegram::envelope::parse_text_entities(text, Some(&value))
        .into_iter()
        .filter(|entity| {
            matches!(
                entity.kind,
                TextEntityKind::Url
                    | TextEntityKind::TextUrl { .. }
                    | TextEntityKind::EmailAddress
                    | TextEntityKind::Mention
                    | TextEntityKind::Hashtag
            )
        })
        .collect()
}

/// Whether `host` is one of Telegram's own (tdesktop `UrlRequiresConfirmation`
/// is false for these), `t.me` included.
fn telegram_host(host: &str) -> bool {
    [
        "telegram.org",
        "telegram.me",
        "telegram.dog",
        "t.me",
        "telegra.ph",
        "te.legra.ph",
        "graph.org",
        "fragment.com",
        "telesco.pe",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// A group's description keeps only links to Telegram's own hosts
/// (tdesktop `StripExternalLinks`); mentions, hashtags and emails stay.
pub fn strip_external_links(text: &str, entities: &[TextEntity]) -> Vec<TextEntity> {
    entities
        .iter()
        .filter(|entity| {
            let target = match &entity.kind {
                TextEntityKind::Url => text.get(entity.utf8_start..entity.utf8_end),
                TextEntityKind::TextUrl { url } => Some(url.as_str()),
                _ => return true,
            };
            target
                .and_then(host_of)
                .is_some_and(|host| telegram_host(&host))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str =
        "Docs https://example.com/a and https://t.me/quill, @quill_app #news me@x.org";

    fn answer(text: &str, spans: &[(&str, &str)]) -> String {
        let entities: Vec<String> = spans
            .iter()
            .filter_map(|(needle, kind)| {
                let byte = text.find(needle)?;
                let offset = text.get(..byte)?.encode_utf16().count();
                let length = needle.encode_utf16().count();
                Some(format!(
                    r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{{"@type":"{kind}"}}}}"#
                ))
            })
            .collect();
        format!(
            r#"{{"@type":"textEntities","entities":[{}]}}"#,
            entities.join(",")
        )
    }

    fn covered<'a>(text: &'a str, entities: &[TextEntity]) -> Vec<&'a str> {
        entities
            .iter()
            .filter_map(|e| text.get(e.utf8_start..e.utf8_end))
            .collect()
    }

    #[test]
    fn a_text_entities_answer_becomes_utf8_entities() {
        let text = "привет https://x.com/a_(b) @quill_app";
        let body = answer(
            text,
            &[
                ("https://x.com/a_(b)", "textEntityTypeUrl"),
                ("@quill_app", "textEntityTypeMention"),
                ("привет", "textEntityTypeBold"),
            ],
        );
        let entities = entities_from_response(text, &body);
        // Bold is not an about entity; the balanced parenthesis stays.
        assert_eq!(
            covered(text, &entities),
            ["https://x.com/a_(b)", "@quill_app"]
        );
    }

    #[test]
    fn a_failed_answer_gives_nothing() {
        assert!(
            entities_from_response("x", r#"{"@type":"error","code":400,"message":"x"}"#).is_empty()
        );
        assert!(entities_from_response("x", "not json").is_empty());
    }

    #[test]
    fn a_group_keeps_only_telegram_links() {
        let body = answer(
            TEXT,
            &[
                ("https://example.com/a", "textEntityTypeUrl"),
                ("https://t.me/quill", "textEntityTypeUrl"),
                ("@quill_app", "textEntityTypeMention"),
                ("#news", "textEntityTypeHashtag"),
                ("me@x.org", "textEntityTypeEmailAddress"),
            ],
        );
        let all = entities_from_response(TEXT, &body);
        assert_eq!(all.len(), 5);
        let kept = strip_external_links(TEXT, &all);
        assert_eq!(
            covered(TEXT, &kept),
            ["https://t.me/quill", "@quill_app", "#news", "me@x.org"]
        );
    }

    #[test]
    fn a_schemeless_or_hidden_external_link_is_stripped_too() {
        let text = "see example.com and t.me/quill";
        let kept = strip_external_links(
            text,
            &[
                TextEntity {
                    utf8_start: 4,
                    utf8_end: 15,
                    kind: TextEntityKind::Url,
                },
                TextEntity {
                    utf8_start: 20,
                    utf8_end: 30,
                    kind: TextEntityKind::Url,
                },
                TextEntity {
                    utf8_start: 0,
                    utf8_end: 3,
                    kind: TextEntityKind::TextUrl {
                        url: "https://evil.example/".into(),
                    },
                },
            ],
        );
        assert_eq!(covered(text, &kept), ["t.me/quill"]);
    }

    /// With a real `libtdjson` the parser itself is checked on the inputs a
    /// whitespace splitter gets wrong; without one the test is skipped.
    #[test]
    fn tdlib_parses_the_hard_cases() {
        let Ok(api) = crate::telegram::ffi::TdJson::load_default() else {
            return;
        };
        let text = "See https://en.wikipedia.org/wiki/Rust_(programming_language) and Node.js, file.txt, @quill's, hi,@quill_app https://x.com。";
        let request = serde_json::json!({ "@type": "getTextEntities", "text": text }).to_string();
        let Ok(Some(body)) = api.execute(&request) else {
            return;
        };
        let entities = entities_from_response(text, &body);
        let spans = covered(text, &entities);
        assert!(spans.contains(&"https://en.wikipedia.org/wiki/Rust_(programming_language)"));
        assert!(spans.contains(&"@quill_app"));
        assert!(!spans.iter().any(|s| s.contains("Node")));
        assert!(spans.contains(&"https://x.com"));
    }
}

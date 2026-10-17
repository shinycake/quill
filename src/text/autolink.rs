//! Links in text Telegram sends without entities: a group or channel
//! description arrives as a plain string (`supergroupFullInfo.description`,
//! `userFullInfo.bio`), so Telegram Desktop parses it on the client
//! (`TextUtilities::ParseEntities`). This is the same job for the kinds the
//! profile shows: web addresses, `@username`, `#hashtag` and email.
#![deny(clippy::indexing_slicing)]

use super::{TextEntity, TextEntityKind, openable_http_url, schemeless_url};

/// Trailing characters that end a sentence rather than a link.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '»'];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The link entities found in `text`, in order, with UTF-8 byte offsets.
pub fn autolink_entities(text: &str) -> Vec<TextEntity> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                push_token(text, s, i, &mut out);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        push_token(text, s, text.len(), &mut out);
    }
    out
}

fn push_token(text: &str, start: usize, end: usize, out: &mut Vec<TextEntity>) {
    let Some(raw) = text.get(start..end) else {
        return;
    };
    // Opening punctuation in front of the token: "(example.com" or "«#tag".
    let lead = raw
        .char_indices()
        .find(|(_, c)| !matches!(c, '(' | '[' | '{' | '"' | '\'' | '«'))
        .map_or(raw.len(), |(i, _)| i);
    let body = raw
        .get(lead..)
        .unwrap_or_default()
        .trim_end_matches(TRAILING);
    if body.is_empty() {
        return;
    }
    let from = start + lead;
    if let Some(kind) = classify(body) {
        out.push(TextEntity {
            utf8_start: from,
            utf8_end: from + body.len(),
            kind,
        });
    }
}

fn classify(body: &str) -> Option<TextEntityKind> {
    if let Some(name) = body.strip_prefix('@') {
        let ok = (4..=32).contains(&name.chars().count()) && name.chars().all(is_word);
        return ok.then_some(TextEntityKind::Mention);
    }
    if let Some(tag) = body.strip_prefix('#') {
        let ok =
            !tag.is_empty() && tag.chars().all(is_word) && !tag.chars().all(|c| c.is_ascii_digit());
        return ok.then_some(TextEntityKind::Hashtag);
    }
    if openable_http_url(body) {
        return Some(TextEntityKind::Url);
    }
    if is_email(body) {
        return Some(TextEntityKind::EmailAddress);
    }
    let host = body.split(['/', '?', '#']).next().unwrap_or_default();
    let tld = host.rsplit('.').next().unwrap_or_default();
    let host_ok = host.contains('.')
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        && !host.starts_with('.')
        && !host.contains("..")
        && tld.chars().count() >= 2
        && tld.chars().all(char::is_alphabetic);
    if !host_ok {
        return None;
    }
    schemeless_url(body).map(|url| TextEntityKind::TextUrl { url })
}

fn is_email(body: &str) -> bool {
    let Some((local, domain)) = body.split_once('@') else {
        return false;
    };
    let tld = domain.rsplit('.').next().unwrap_or_default();
    !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
        && domain.contains('.')
        && !domain.contains('@')
        && domain
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        && tld.chars().count() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(String, TextEntityKind)> {
        autolink_entities(text)
            .into_iter()
            .filter_map(|e| Some((text.get(e.utf8_start..e.utf8_end)?.to_string(), e.kind)))
            .collect()
    }

    #[test]
    fn finds_urls_mentions_hashtags_and_emails() {
        let got = found("Docs https://example.com/a?b=1, chat @quill_app #news mail me@x.org.");
        let spans: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(
            spans,
            [
                "https://example.com/a?b=1",
                "@quill_app",
                "#news",
                "me@x.org"
            ]
        );
        let kinds: Vec<&TextEntityKind> = got.iter().map(|(_, k)| k).collect();
        assert_eq!(
            kinds,
            [
                &TextEntityKind::Url,
                &TextEntityKind::Mention,
                &TextEntityKind::Hashtag,
                &TextEntityKind::EmailAddress
            ]
        );
    }

    #[test]
    fn a_bare_domain_opens_over_https() {
        let got = found("see (t.me/quill).");
        assert_eq!(
            got,
            [(
                "t.me/quill".to_string(),
                TextEntityKind::TextUrl {
                    url: "https://t.me/quill".into()
                }
            )]
        );
    }

    #[test]
    fn plain_words_and_versions_are_left_alone() {
        assert!(found("Version 1.5 is out. Hello, world! #123 @ab").is_empty());
    }

    #[test]
    fn offsets_follow_multibyte_text() {
        let text = "привет @quill_app";
        let entity = autolink_entities(text).pop().expect("one mention");
        assert_eq!(
            text.get(entity.utf8_start..entity.utf8_end),
            Some("@quill_app")
        );
    }
}

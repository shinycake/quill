//! UTF-16 entity offsets used by TDLib, mapped to UTF-8 byte indices.

/// Convert a UTF-8 byte index into a UTF-16 code-unit offset.
///
/// The byte index must land on a character boundary. Surrogate pairs (non-BMP
/// code points, including many emoji) count as two UTF-16 units.
pub fn utf8_to_utf16_offset(text: &str, byte_index: usize) -> Result<i32, OffsetError> {
    if byte_index > text.len() {
        return Err(OffsetError::OutOfRange);
    }
    if byte_index < text.len() && !text.is_char_boundary(byte_index) {
        return Err(OffsetError::NotCharBoundary);
    }
    let mut units = 0i32;
    for (i, ch) in text.char_indices() {
        if i == byte_index {
            return Ok(units);
        }
        if i > byte_index {
            return Err(OffsetError::NotCharBoundary);
        }
        units = units.saturating_add(ch.len_utf16() as i32);
    }
    if byte_index == text.len() {
        Ok(units)
    } else {
        Err(OffsetError::NotCharBoundary)
    }
}

/// Convert a UTF-16 code-unit offset into a UTF-8 byte index.
pub fn utf16_to_utf8_offset(text: &str, utf16_offset: i32) -> Result<usize, OffsetError> {
    if utf16_offset < 0 {
        return Err(OffsetError::OutOfRange);
    }
    let mut remaining = utf16_offset as usize;
    for (i, ch) in text.char_indices() {
        if remaining == 0 {
            return Ok(i);
        }
        let width = ch.len_utf16();
        if remaining < width {
            // Offset landed inside a surrogate pair.
            return Err(OffsetError::InsideSurrogatePair);
        }
        remaining -= width;
    }
    if remaining == 0 {
        Ok(text.len())
    } else {
        Err(OffsetError::OutOfRange)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    OutOfRange,
    NotCharBoundary,
    InsideSurrogatePair,
}

/// True when `needle` appears as a contiguous substring (used by log canaries).
pub fn contains_canary(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

/// Styled text entities Quill paints in message text and captions (Phase 4.1).
///
/// TDLib `textEntity` offsets are UTF-16. Callers convert them to UTF-8 byte
/// indices before storing a span. Entity type constructors are verified
/// against `schema/td_api.tl` (1.8.67, lines 5719–5785); anything not listed
/// here stays unparsed and unstyled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEntityKind {
    /// `textEntityTypeUrl` — the substring is the HTTP URL.
    Url,
    /// `textEntityTypeTextUrl` — visible label, `url` is opened on click.
    TextUrl { url: String },
    /// `textEntityTypeBold`
    Bold,
    /// `textEntityTypeItalic`
    Italic,
    /// `textEntityTypeUnderline`
    Underline,
    /// `textEntityTypeStrikethrough`
    Strikethrough,
    /// `textEntityTypeSpoiler` — hidden until tapped.
    Spoiler,
    /// `textEntityTypeCode` — inline monospace chip.
    Code,
    /// `textEntityTypePre` — monospace block, no language.
    Pre,
    /// `textEntityTypePreCode language:string` — monospace block with language.
    PreCode { language: String },
    /// `textEntityTypeBlockQuote` — quote block; collapses past
    /// `QUOTE_COLLAPSE_LINES` lines.
    BlockQuote,
    /// `textEntityTypeExpandableBlockQuote` — quote block; same collapse
    /// behavior as `BlockQuote` (kept as its own variant for schema fidelity).
    ExpandableBlockQuote,
    /// `textEntityTypeCustomEmoji custom_emoji_id:int64` — the covered span
    /// renders as the custom emoji's sticker image; the span text stays as
    /// the fallback when the sticker isn't resolved.
    CustomEmoji { custom_emoji_id: i64 },
    /// `textEntityTypeMention` — `@username`; opens the chat or profile.
    Mention,
    /// `textEntityTypeMentionName user_id:int53` — a name that links to a
    /// user without a username; opens the profile.
    MentionName { user_id: i64 },
    /// `textEntityTypeHashtag` — `#tag`; searches for it.
    Hashtag,
    /// `textEntityTypeCashtag` — `$USD`; searches for it.
    Cashtag,
    /// `textEntityTypeBotCommand` — `/command` or `/command@bot`.
    BotCommand,
    /// `textEntityTypeEmailAddress` — opens the mail client.
    EmailAddress,
    /// `textEntityTypePhoneNumber` — offers copy.
    PhoneNumber,
    /// `textEntityTypeBankCardNumber` — offers copy.
    BankCardNumber,
    /// `textEntityTypeMediaTimestamp media_timestamp:int32` — seeks the
    /// message's (or the replied message's) audio or video.
    MediaTimestamp { seconds: i32 },
    /// `textEntityTypeDateTime unix_time:int32` — offers copy of the date.
    DateTime { unix_time: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntity {
    pub utf8_start: usize,
    pub utf8_end: usize,
    pub kind: TextEntityKind,
}

impl TextEntity {
    /// The click target of a non-URL interactive entity over `text`.
    pub fn interactive_target(&self, text: &str) -> Option<LinkTarget> {
        let covered = || text.get(self.utf8_start..self.utf8_end).map(str::to_string);
        Some(match &self.kind {
            TextEntityKind::Mention => LinkTarget::Mention(covered()?),
            TextEntityKind::MentionName { user_id } => LinkTarget::MentionName {
                user_id: *user_id,
                label: covered()?,
            },
            TextEntityKind::Hashtag => LinkTarget::Hashtag(covered()?),
            TextEntityKind::Cashtag => LinkTarget::Cashtag(covered()?),
            TextEntityKind::BotCommand => LinkTarget::BotCommand(covered()?),
            TextEntityKind::EmailAddress => LinkTarget::Email(covered()?),
            TextEntityKind::PhoneNumber => LinkTarget::Phone(covered()?),
            TextEntityKind::BankCardNumber => LinkTarget::BankCard(covered()?),
            TextEntityKind::MediaTimestamp { seconds } => {
                LinkTarget::MediaTimestamp { seconds: *seconds }
            }
            TextEntityKind::DateTime { unix_time } => LinkTarget::DateTime {
                unix_time: *unix_time,
                label: covered()?,
            },
            _ => return None,
        })
    }

    /// URL passed to the OS opener. Only `http` and `https` (no shell).
    pub fn open_href<'a>(&'a self, text: &'a str) -> Option<&'a str> {
        let raw = match &self.kind {
            TextEntityKind::Url => text.get(self.utf8_start..self.utf8_end)?,
            TextEntityKind::TextUrl { url } => url.as_str(),
            // Style entities carry no link target.
            _ => return None,
        };
        openable_http_url(raw).then_some(raw.trim())
    }
}

/// What a clickable entity does (everything `TextEntityKind` marks as
/// interactive). Carried by [`TextRun::link`] so the UI can act on a click,
/// build the link context menu and decide whether to confirm a hidden URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// An `http(s)` link. `label` is the visible text of a `textUrl`
    /// (`None` for a bare `textEntityTypeUrl`, whose text is the URL).
    Url {
        url: String,
        label: Option<String>,
    },
    /// `@username`, including the `@`.
    Mention(String),
    MentionName {
        user_id: i64,
        label: String,
    },
    /// `#tag`, including the `#`.
    Hashtag(String),
    /// `$TAG`, including the `$`.
    Cashtag(String),
    /// `/command` or `/command@bot`, including the `/`.
    BotCommand(String),
    Email(String),
    Phone(String),
    BankCard(String),
    MediaTimestamp {
        seconds: i32,
    },
    DateTime {
        unix_time: i32,
        label: String,
    },
}

impl LinkTarget {
    /// The text "Copy …" puts on the clipboard.
    pub fn copy_text(&self) -> Option<&str> {
        match self {
            Self::Url { url, .. } => Some(url),
            Self::Mention(text)
            | Self::Hashtag(text)
            | Self::Cashtag(text)
            | Self::Email(text)
            | Self::Phone(text)
            | Self::BankCard(text)
            | Self::BotCommand(text) => Some(text),
            Self::MentionName { .. } | Self::MediaTimestamp { .. } | Self::DateTime { .. } => None,
        }
    }

    /// Telegram Desktop's context-menu label for copying this link
    /// (`lng_context_copy_link` / `_mention` / `_hashtag` / `_email`).
    pub fn copy_label(&self) -> Option<&'static str> {
        match self {
            Self::Url { .. } => Some("Copy Link"),
            Self::Mention(_) => Some("Copy Username"),
            Self::Hashtag(_) | Self::Cashtag(_) => Some("Copy Hashtag"),
            Self::Email(_) => Some("Copy Email Address"),
            Self::Phone(_) => Some("Copy Phone Number"),
            Self::BankCard(_) => Some("Copy Card Number"),
            Self::BotCommand(_) => Some("Copy Command"),
            Self::MentionName { .. } | Self::MediaTimestamp { .. } | Self::DateTime { .. } => None,
        }
    }

    /// Tooltip text on hover (Telegram Desktop `ClickHandler::tooltip`):
    /// the URL of a hidden `textUrl`, the number of a phone link, and the
    /// date of a `dateTime` is left out (its text already shows it).
    pub fn tooltip(&self) -> Option<String> {
        match self {
            Self::Url {
                url,
                label: Some(label),
            } if label.trim() != url.trim() => Some(url.clone()),
            Self::Phone(number) => Some(number.clone()),
            _ => None,
        }
    }
}

/// One painted slice of message text. `href` is set for clickable links;
/// `style` carries the Phase 4.1 entity styling for the same slice;
/// `custom_emoji_id` is set when the slice is a custom emoji (rendered as
/// the sticker image, falling back to `text`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub href: Option<String>,
    /// The interactive entity covering the slice, if any (a superset of
    /// `href`, which only holds openable web links).
    pub link: Option<LinkTarget>,
    pub style: RunStyle,
    pub custom_emoji_id: Option<i64>,
}

/// Combined styling for one painted run (Phase 4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Spoiler: hidden until tapped; the UI keeps reveal state.
    pub spoiler: bool,
    /// Inline `code`: monospace chip.
    pub code: bool,
    /// `pre` / `preCode`: monospace block.
    pub pre: bool,
    /// Language from `textEntityTypePreCode`; `None` for plain `pre`.
    pub language: Option<String>,
    /// Block quote (`textEntityTypeBlockQuote` /
    /// `textEntityTypeExpandableBlockQuote`) — renders as a quote block;
    /// long quotes collapse with an expand affordance.
    pub quote: bool,
}

impl RunStyle {
    /// True when the run carries no entity styling at all.
    pub fn is_plain(&self) -> bool {
        self == &RunStyle::default()
    }
}

/// Split `text` into painted runs honoring every entity, nested or not.
///
/// Nesting rule (Phase 4.1): the text is cut at every entity boundary, so
/// each emitted run is covered by one fixed set of entities. Styles combine
/// additively across nested / partially overlapping entities (bold inside
/// italic renders bold italic; `code` inside a spoiler renders a hidden code
/// chip; `pre` inside a spoiler renders a hidden block). Adjacent runs with
/// identical styling are merged back together.
///
/// Degradation rules, all deterministic and panic-free:
/// - `href`: the entity with the smallest `(utf8_start, utf8_end)` wins per
///   run. The schema forbids Url / TextUrl nesting, so this only matters for
///   malformed input.
/// - `custom_emoji_id`: same smallest-wins rule; runs with different ids
///   never merge (each renders its own sticker image).
/// - `pre` wins over `code` for the block look; both stay monospace.
/// - Degenerate entities (zero length, outside the text, or splitting a
///   UTF-8 char boundary) are dropped.
pub fn styled_runs(text: &str, entities: &[TextEntity]) -> Vec<TextRun> {
    let mut spans: Vec<&TextEntity> = entities
        .iter()
        .filter(|entity| {
            entity.utf8_start < entity.utf8_end
                && entity.utf8_end <= text.len()
                && text.is_char_boundary(entity.utf8_start)
                && text.is_char_boundary(entity.utf8_end)
        })
        .collect();
    spans.sort_by_key(|entity| (entity.utf8_start, entity.utf8_end));
    let mut points = vec![0usize, text.len()];
    for entity in &spans {
        points.push(entity.utf8_start);
        points.push(entity.utf8_end);
    }
    points.sort_unstable();
    points.dedup();
    let mut runs: Vec<TextRun> = Vec::new();
    for window in points.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let mut style = RunStyle::default();
        let mut href: Option<String> = None;
        let mut link: Option<LinkTarget> = None;
        let mut custom_emoji_id: Option<i64> = None;
        for entity in &spans {
            if entity.utf8_start > start || entity.utf8_end < end {
                continue;
            }
            match &entity.kind {
                TextEntityKind::Url | TextEntityKind::TextUrl { .. } => {
                    if href.is_none() {
                        href = entity.open_href(text).map(str::to_string).or_else(|| {
                            // A bare `example.com` entity has no scheme;
                            // Telegram Desktop opens it as `http://…`.
                            matches!(entity.kind, TextEntityKind::Url)
                                .then(|| schemeless_url(&text[entity.utf8_start..entity.utf8_end]))
                                .flatten()
                        });
                        if link.is_none() {
                            link = href.as_ref().map(|url| LinkTarget::Url {
                                url: url.clone(),
                                label: matches!(entity.kind, TextEntityKind::TextUrl { .. })
                                    .then(|| text[entity.utf8_start..entity.utf8_end].to_string()),
                            });
                        }
                    }
                }
                TextEntityKind::Mention
                | TextEntityKind::MentionName { .. }
                | TextEntityKind::Hashtag
                | TextEntityKind::Cashtag
                | TextEntityKind::BotCommand
                | TextEntityKind::EmailAddress
                | TextEntityKind::PhoneNumber
                | TextEntityKind::BankCardNumber
                | TextEntityKind::MediaTimestamp { .. }
                | TextEntityKind::DateTime { .. } => {
                    if link.is_none() {
                        link = entity.interactive_target(text);
                    }
                }
                TextEntityKind::CustomEmoji {
                    custom_emoji_id: id,
                } => {
                    if custom_emoji_id.is_none() {
                        custom_emoji_id = Some(*id);
                    }
                }
                TextEntityKind::Bold => style.bold = true,
                TextEntityKind::Italic => style.italic = true,
                TextEntityKind::Underline => style.underline = true,
                TextEntityKind::Strikethrough => style.strikethrough = true,
                TextEntityKind::Spoiler => style.spoiler = true,
                TextEntityKind::Code => style.code = true,
                TextEntityKind::Pre => style.pre = true,
                TextEntityKind::PreCode { language } => {
                    style.pre = true;
                    if style.language.is_none() {
                        style.language = Some(language.clone());
                    }
                }
                TextEntityKind::BlockQuote | TextEntityKind::ExpandableBlockQuote => {
                    style.quote = true;
                }
            }
        }
        let slice = text[start..end].to_string();
        if let Some(last) = runs.last_mut()
            && last.style == style
            && last.href == href
            && last.link == link
            && last.custom_emoji_id == custom_emoji_id
        {
            last.text.push_str(&slice);
        } else {
            runs.push(TextRun {
                text: slice,
                href,
                link,
                style,
                custom_emoji_id,
            });
        }
    }
    runs
}

/// Block quotes collapse past this many visible lines. TDLib documents
/// `textEntityTypeExpandableBlockQuote` as "collapsed by default to 3 lines
/// with the ability to show full text" (`schema/td_api.tl:5770`); long plain
/// `textEntityTypeBlockQuote`s collapse the same way so both render
/// identically.
pub const QUOTE_COLLAPSE_LINES: usize = 3;

/// True when a block quote's text is long enough to collapse: strictly more
/// lines than [`QUOTE_COLLAPSE_LINES`].
pub fn quote_collapses(quote_text: &str) -> bool {
    quote_text.lines().count() > QUOTE_COLLAPSE_LINES
}

/// Byte length of the first three logical lines, measured before style runs split them.
pub fn collapsed_quote_len(text: &str) -> usize {
    text.match_indices('\n')
        .nth(QUOTE_COLLAPSE_LINES - 1)
        .map_or(text.len(), |(index, _)| index)
}

/// `https://` + `raw` for a scheme-less web address such as `example.com/a`
/// (a `textEntityTypeUrl` carries the text as written); `None` when `raw`
/// already has a scheme, has no dot, or would not be an openable link.
pub fn schemeless_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.contains("://") || raw.contains('@') || !raw.contains('.') {
        return None;
    }
    let url = format!("https://{raw}");
    openable_http_url(&url).then_some(url)
}

/// `http`/`https` only, no whitespace or control characters (passed to xdg-open/open).
pub fn openable_http_url(url: &str) -> bool {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    !rest.is_empty()
        && url
            .chars()
            .all(|ch| !ch.is_whitespace() && !ch.is_control())
}

/// Whether `text` reads right to left: its first character with a strong
/// direction is Hebrew, Arabic or another RTL script (rules P2/P3 of the
/// Unicode Bidirectional Algorithm, the "auto" direction Telegram Desktop's
/// text and input field use to align a paragraph). Digits, punctuation and
/// emoji are neutral, so a paragraph made only of them is left to right.
pub fn is_rtl_text(text: &str) -> bool {
    unicode_bidi::get_base_direction_full(text) == unicode_bidi::Direction::Rtl
}

/// Whether `text` holds a right-to-left character (or one that starts a
/// right-to-left run), so it needs bidirectional layout: wrapping in typing
/// order and visual reordering per row. Plain left-to-right text, ASCII
/// included, does not.
pub fn has_rtl_text(text: &str) -> bool {
    use unicode_bidi::BidiClass::{AL, AN, FSI, R, RLE, RLI, RLO};
    !text.is_ascii()
        && text.chars().any(|c| {
            matches!(
                unicode_bidi::bidi_class(c),
                R | AL | AN | RLE | RLO | RLI | FSI
            )
        })
}

/// Direction of the last paragraph of `text` (where a message's time footer
/// sits). Telegram Desktop (`Ui::Text::String::recountNaturalSize`,
/// `_endsWithQuoteOrOtherDirection`) puts the footer on its own line when the
/// *last* paragraph's direction differs from the UI's; a last line with no
/// strong character has no direction of its own, so it shares the line with the
/// time even after a Hebrew paragraph.
pub fn last_line_is_rtl(text: &str) -> bool {
    text.trim_end().lines().next_back().is_some_and(is_rtl_text)
}

#[cfg(test)]
mod tests;

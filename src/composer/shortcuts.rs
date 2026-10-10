//! Formatting shortcuts (tdesktop chords) and link editing.

use super::*;

/// What a composer formatting shortcut does (Telegram Desktop's
/// `InputField::setupMarkdownShortcuts`, lib_ui/ui/widgets/fields/
/// input_field.cpp:2041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerShortcut {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    /// Inline code, or a code block when the selection spans lines.
    Monospace,
    BlockQuote,
    Spoiler,
    ClearFormatting,
    EditLink,
}

/// One row of the shortcut table: the chord is the platform's primary
/// modifier (Cmd on macOS, Ctrl elsewhere; Qt's `ctrl` maps the same way)
/// plus `keys`.
#[derive(Debug, Clone, Copy)]
pub struct ComposerShortcutSpec {
    pub shortcut: ComposerShortcut,
    /// Menu label.
    pub label: &'static str,
    /// Whether Shift is part of the chord.
    pub shift: bool,
    /// The key, lower case (`"b"`, `"."`).
    pub key: &'static str,
}

/// tdesktop's table: Bold/Italic/Underline are `QKeySequence::Bold` etc.
/// (Ctrl+B/I/U); the rest are `kStrikeOutSequence` ctrl+shift+x,
/// `kMonospaceSequence` ctrl+shift+m, `kBlockquoteSequence` ctrl+shift+.,
/// `kSpoilerSequence` ctrl+shift+p, `kClearFormatSequence` ctrl+shift+n and
/// `kEditLinkSequence` ctrl+k (input_field.h:40).
pub const COMPOSER_SHORTCUTS: &[ComposerShortcutSpec] = &[
    spec(ComposerShortcut::Bold, "Bold", false, "b"),
    spec(ComposerShortcut::Italic, "Italic", false, "i"),
    spec(ComposerShortcut::Underline, "Underline", false, "u"),
    spec(ComposerShortcut::Strikethrough, "Strikethrough", true, "x"),
    spec(ComposerShortcut::BlockQuote, "Quote", true, "."),
    spec(ComposerShortcut::Monospace, "Monospace", true, "m"),
    spec(ComposerShortcut::Spoiler, "Spoiler", true, "p"),
    spec(
        ComposerShortcut::ClearFormatting,
        "Clear formatting",
        true,
        "n",
    ),
    spec(ComposerShortcut::EditLink, "Link", false, "k"),
];

const fn spec(
    shortcut: ComposerShortcut,
    label: &'static str,
    shift: bool,
    key: &'static str,
) -> ComposerShortcutSpec {
    ComposerShortcutSpec {
        shortcut,
        label,
        shift,
        key,
    }
}

/// The shortcut for a primary-modifier chord (`shift`, lower-case `key`).
pub fn composer_shortcut_for(shift: bool, key: &str) -> Option<ComposerShortcut> {
    COMPOSER_SHORTCUTS
        .iter()
        .find(|spec| spec.shift == shift && spec.key.eq_ignore_ascii_case(key))
        .map(|spec| spec.shortcut)
}

impl ComposerShortcut {
    /// The formatting this shortcut applies to `selected` (the selected
    /// text), or `None` for clear-formatting and the link dialog. Monospace
    /// follows tdesktop: one line is inline code, several are a code block.
    pub fn format_action(self, selected: &str) -> Option<FormatAction> {
        Some(match self {
            ComposerShortcut::Bold => FormatAction::Bold,
            ComposerShortcut::Italic => FormatAction::Italic,
            ComposerShortcut::Underline => FormatAction::Underline,
            ComposerShortcut::Strikethrough => FormatAction::Strikethrough,
            ComposerShortcut::BlockQuote => FormatAction::BlockQuote,
            ComposerShortcut::Spoiler => FormatAction::Spoiler,
            ComposerShortcut::Monospace if selected.contains('\n') => FormatAction::Pre,
            ComposerShortcut::Monospace => FormatAction::Code,
            ComposerShortcut::ClearFormatting | ComposerShortcut::EditLink => return None,
        })
    }
}

/// `Cmd+K` in the composer: with a selection it edits a link (tdesktop
/// `executeMarkdownAction(EditLink)`); without one the chord keeps its
/// app-wide meaning, quick switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkChord {
    EditLink,
    QuickSwitch,
}

pub fn link_chord_target(composer_focused: bool, has_selection: bool) -> LinkChord {
    if composer_focused && has_selection {
        LinkChord::EditLink
    } else {
        LinkChord::QuickSwitch
    }
}

/// Link target as typed in the link dialog: trimmed, and `https://` is
/// assumed when there is no scheme (tdesktop `AutoValidateLink`). Empty
/// input means "no link".
pub fn normalize_link_url(input: &str) -> Option<String> {
    let url = input.trim();
    if url.is_empty() || url.chars().any(char::is_whitespace) {
        return None;
    }
    let has_scheme = url.split_once(':').is_some_and(|(scheme, rest)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
            && (rest.starts_with("//")
                || scheme.eq_ignore_ascii_case("mailto")
                || scheme.eq_ignore_ascii_case("tg"))
    });
    Some(if has_scheme {
        url.to_string()
    } else {
        format!("https://{url}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_map_to_tdesktop_entities() {
        let cases = [
            (false, "b", ComposerShortcut::Bold),
            (false, "i", ComposerShortcut::Italic),
            (false, "u", ComposerShortcut::Underline),
            (true, "x", ComposerShortcut::Strikethrough),
            (true, "m", ComposerShortcut::Monospace),
            (true, ".", ComposerShortcut::BlockQuote),
            (true, "p", ComposerShortcut::Spoiler),
            (true, "n", ComposerShortcut::ClearFormatting),
            (false, "k", ComposerShortcut::EditLink),
        ];
        for (shift, key, expected) in cases {
            assert_eq!(composer_shortcut_for(shift, key), Some(expected), "{key}");
        }
        // Shift state matters: Cmd+X is cut, Cmd+Shift+B is nothing.
        assert_eq!(composer_shortcut_for(false, "x"), None);
        assert_eq!(composer_shortcut_for(true, "b"), None);
        assert_eq!(COMPOSER_SHORTCUTS.len(), cases.len());
    }

    #[test]
    fn quote_prefixes_a_fully_selected_single_line() {
        let (text, sel) = super::apply_format_markup("hello", 0..5, &FormatAction::BlockQuote);
        assert_eq!(text, "> hello");
        assert_eq!(sel, 0..7);
        let (text, _) = super::apply_format_markup("hello", 5..5, &FormatAction::BlockQuote);
        assert_eq!(text, "> hello");
    }

    #[test]
    fn shortcuts_apply_the_matching_markup() {
        assert_eq!(
            ComposerShortcut::Bold.format_action("x"),
            Some(FormatAction::Bold)
        );
        assert_eq!(
            ComposerShortcut::Spoiler.format_action(""),
            Some(FormatAction::Spoiler)
        );
        assert_eq!(
            ComposerShortcut::BlockQuote.format_action(""),
            Some(FormatAction::BlockQuote)
        );
        assert_eq!(ComposerShortcut::ClearFormatting.format_action("x"), None);
        assert_eq!(ComposerShortcut::EditLink.format_action("x"), None);
    }

    #[test]
    fn monospace_is_inline_for_a_line_and_a_block_for_many() {
        assert_eq!(
            ComposerShortcut::Monospace.format_action("one line"),
            Some(FormatAction::Code)
        );
        assert_eq!(
            ComposerShortcut::Monospace.format_action("two\nlines"),
            Some(FormatAction::Pre)
        );
    }

    #[test]
    fn cmd_k_edits_a_link_only_inside_the_composer_with_a_selection() {
        assert_eq!(link_chord_target(true, true), LinkChord::EditLink);
        assert_eq!(link_chord_target(true, false), LinkChord::QuickSwitch);
        assert_eq!(link_chord_target(false, true), LinkChord::QuickSwitch);
        assert_eq!(link_chord_target(false, false), LinkChord::QuickSwitch);
    }

    #[test]
    fn link_urls_get_a_scheme() {
        assert_eq!(
            normalize_link_url(" example.com/a "),
            Some("https://example.com/a".into())
        );
        assert_eq!(
            normalize_link_url("http://example.com"),
            Some("http://example.com".into())
        );
        assert_eq!(
            normalize_link_url("mailto:a@b.co"),
            Some("mailto:a@b.co".into())
        );
        assert_eq!(
            normalize_link_url("tg:resolve?domain=x"),
            Some("tg:resolve?domain=x".into())
        );
        assert_eq!(normalize_link_url("   "), None);
        assert_eq!(normalize_link_url("two words"), None);
    }
}

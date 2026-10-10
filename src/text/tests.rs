use super::*;

#[test]
fn a_card_number_entity_offers_copy_card_number() {
    let text = "Pay to 4111 1111 1111 1111 today";
    let entity = TextEntity {
        utf8_start: 7,
        utf8_end: 26,
        kind: TextEntityKind::BankCardNumber,
    };
    let link = entity.interactive_target(text).expect("a card link");
    assert_eq!(link.copy_label(), Some("Copy Card Number"));
    assert_eq!(link.copy_text(), Some("4111 1111 1111 1111"));
}

#[test]
fn ascii_round_trip() {
    let text = "hello";
    assert_eq!(utf8_to_utf16_offset(text, 0).unwrap(), 0);
    assert_eq!(utf8_to_utf16_offset(text, 5).unwrap(), 5);
    assert_eq!(utf16_to_utf8_offset(text, 2).unwrap(), 2);
}

#[test]
fn hebrew_and_latin() {
    let text = "שלום hello";
    let hello_start = text.find("hello").unwrap();
    let utf16 = utf8_to_utf16_offset(text, hello_start).unwrap();
    assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), hello_start);
    assert_eq!(utf8_to_utf16_offset(text, text.len()).unwrap(), 10);
}

#[test]
fn arabic_digits_and_link() {
    let text = "مرحبا https://example.com 123";
    let link = text.find("https").unwrap();
    let utf16 = utf8_to_utf16_offset(text, link).unwrap();
    assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), link);
}

#[test]
fn emoji_family_zwj_is_non_bmp() {
    // Woman+ZWJ+woman+ZWJ+girl — several non-BMP scalars.
    let text = "hello 👩‍👧‍👦 world";
    let emoji_at = text.find('👩').unwrap();
    let utf16 = utf8_to_utf16_offset(text, emoji_at).unwrap();
    assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), emoji_at);
    let after = text.find(" world").unwrap();
    let utf16_after = utf8_to_utf16_offset(text, after).unwrap();
    assert_eq!(utf16_to_utf8_offset(text, utf16_after).unwrap(), after);
    assert!(utf16_after > utf16 + 1);
}

#[test]
fn skin_tone_and_combining_accent() {
    let text = "e\u{0301} 👍🏽";
    assert_eq!(
        utf16_to_utf8_offset(text, utf8_to_utf16_offset(text, 0).unwrap()).unwrap(),
        0
    );
    let thumb = text.find('👍').unwrap();
    let utf16 = utf8_to_utf16_offset(text, thumb).unwrap();
    assert_eq!(utf16_to_utf8_offset(text, utf16).unwrap(), thumb);
}

#[test]
fn cjk_offsets() {
    let text = "日本語入力";
    assert_eq!(utf8_to_utf16_offset(text, text.len()).unwrap(), 5);
    assert_eq!(utf16_to_utf8_offset(text, 2).unwrap(), "日本".len());
}

#[test]
fn rejects_mid_char_byte_index() {
    let text = "é";
    assert_eq!(
        utf8_to_utf16_offset(text, 1),
        Err(OffsetError::NotCharBoundary)
    );
}

#[test]
fn rejects_offset_inside_surrogate_pair() {
    let text = "𝄞"; // U+1D11E, two UTF-16 units, four UTF-8 bytes
    assert_eq!(
        utf16_to_utf8_offset(text, 1),
        Err(OffsetError::InsideSurrogatePair)
    );
}

#[test]
fn url_and_text_url_runs_use_utf8_spans() {
    let text = "see https://example.com now";
    let start = text.find("https").unwrap();
    let end = start + "https://example.com".len();
    let entities = vec![
        entity(start, end, TextEntityKind::Url),
        entity(
            text.find("now").unwrap(),
            text.len(),
            TextEntityKind::TextUrl {
                url: "https://example.com/notes".into(),
            },
        ),
    ];
    let runs = styled_runs(text, &entities);
    assert_eq!(runs[0].text, "see ");
    assert!(runs[0].href.is_none());
    assert!(runs[0].style.is_plain());
    assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
    assert_eq!(runs[2].text, " ");
    assert_eq!(runs[3].text, "now");
    assert_eq!(runs[3].href.as_deref(), Some("https://example.com/notes"));
}

#[test]
fn emoji_prefix_does_not_shift_link_bytes() {
    let text = "👋 https://example.com";
    let start = text.find("https").unwrap();
    let runs = styled_runs(text, &[entity(start, text.len(), TextEntityKind::Url)]);
    assert_eq!(runs[0].text, "👋 ");
    assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
}

#[test]
fn rejects_non_http_and_first_link_wins_overlap() {
    let text = "javascript:alert(1) hi";
    let bad = entity(0, "javascript:alert(1)".len(), TextEntityKind::Url);
    let runs = styled_runs(text, &[bad]);
    assert!(runs.iter().all(|run| run.href.is_none()));
    assert!(!openable_http_url("javascript:alert(1)"));
    assert!(!openable_http_url("https://evil.com/a b"));
    assert!(openable_http_url("https://example.com/a"));

    // Overlapping links (forbidden by the schema) degrade deterministically:
    // the entity with the smallest (start, end) wins each run.
    let overlap = [
        entity(
            0,
            4,
            TextEntityKind::TextUrl {
                url: "https://a.example".into(),
            },
        ),
        entity(
            2,
            6,
            TextEntityKind::TextUrl {
                url: "https://b.example".into(),
            },
        ),
    ];
    let runs = styled_runs("abcdef", &overlap);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "abcd");
    assert_eq!(runs[0].href.as_deref(), Some("https://a.example"));
    assert_eq!(runs[1].text, "ef");
    assert_eq!(runs[1].href.as_deref(), Some("https://b.example"));
}

fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
    TextEntity {
        utf8_start: start,
        utf8_end: end,
        kind,
    }
}

#[test]
fn styled_runs_custom_emoji_carries_id_and_never_merges_across_ids() {
    // "a😀😃b": a=0..1, 😀=1..5, 😃=5..9, b=9..10 (UTF-8).
    let runs = styled_runs(
        "a😀😃b",
        &[
            entity(
                1,
                5,
                TextEntityKind::CustomEmoji {
                    custom_emoji_id: 11,
                },
            ),
            entity(
                5,
                9,
                TextEntityKind::CustomEmoji {
                    custom_emoji_id: 22,
                },
            ),
        ],
    );
    assert_eq!(runs.len(), 4);
    assert!(runs[0].custom_emoji_id.is_none());
    assert_eq!(runs[1].custom_emoji_id, Some(11));
    assert_eq!(runs[2].custom_emoji_id, Some(22));
    assert!(runs[3].custom_emoji_id.is_none());
}

#[test]
fn styled_runs_custom_emoji_same_id_merges() {
    let runs = styled_runs(
        "😀😀",
        &[
            entity(
                0,
                4,
                TextEntityKind::CustomEmoji {
                    custom_emoji_id: 11,
                },
            ),
            entity(
                4,
                8,
                TextEntityKind::CustomEmoji {
                    custom_emoji_id: 11,
                },
            ),
        ],
    );
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].custom_emoji_id, Some(11));
}

#[test]
fn styled_runs_plain_text_is_one_run() {
    let runs = styled_runs("hello", &[]);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "hello");
    assert!(runs[0].style.is_plain());
    assert!(runs[0].href.is_none());
}

#[test]
fn styled_runs_single_style_segments() {
    let text = "bold and italic";
    let runs = styled_runs(
        text,
        &[
            entity(0, 4, TextEntityKind::Bold),
            entity(9, 15, TextEntityKind::Italic),
        ],
    );
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].text, "bold");
    assert!(runs[0].style.bold);
    assert_eq!(runs[1].text, " and ");
    assert!(runs[1].style.is_plain());
    assert_eq!(runs[2].text, "italic");
    assert!(runs[2].style.italic);
}

#[test]
fn styled_runs_nested_styles_combine() {
    // "abcdef": bold over 0..6, italic over 2..4, strikethrough over 3..6.
    let text = "abcdef";
    let runs = styled_runs(
        text,
        &[
            entity(0, 6, TextEntityKind::Bold),
            entity(2, 4, TextEntityKind::Italic),
            entity(3, 6, TextEntityKind::Strikethrough),
        ],
    );
    assert_eq!(runs.len(), 4);
    assert_eq!(runs[0].text, "ab");
    assert!(runs[0].style.bold && !runs[0].style.italic);
    assert_eq!(runs[1].text, "c");
    assert!(runs[1].style.bold && runs[1].style.italic);
    assert_eq!(runs[2].text, "d");
    assert!(runs[2].style.bold && runs[2].style.italic && runs[2].style.strikethrough);
    assert_eq!(runs[3].text, "ef");
    assert!(runs[3].style.bold && runs[3].style.strikethrough && !runs[3].style.italic);
}

#[test]
fn styled_runs_partial_overlap_splits_and_merges() {
    // bold 0..4, italic 2..6: runs are 0..2 bold, 2..4 bold+italic,
    // 4..6 italic.
    let runs = styled_runs(
        "abcdef",
        &[
            entity(0, 4, TextEntityKind::Bold),
            entity(2, 6, TextEntityKind::Italic),
        ],
    );
    assert_eq!(runs.len(), 3);
    assert!(runs[0].style.bold && !runs[0].style.italic);
    assert!(runs[1].style.bold && runs[1].style.italic);
    assert!(!runs[2].style.bold && runs[2].style.italic);
}

#[test]
fn styled_runs_adjacent_same_style_merges() {
    let runs = styled_runs(
        "abcd",
        &[
            entity(0, 2, TextEntityKind::Bold),
            entity(2, 4, TextEntityKind::Bold),
        ],
    );
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "abcd");
    assert!(runs[0].style.bold);
}

#[test]
fn styled_runs_code_pre_and_language() {
    let runs = styled_runs(
        "a b c",
        &[
            entity(2, 3, TextEntityKind::Code),
            entity(4, 5, TextEntityKind::Pre),
        ],
    );
    assert_eq!(runs.len(), 4);
    assert_eq!(runs[0].text, "a ");
    assert!(runs[0].style.is_plain());
    assert_eq!(runs[1].text, "b");
    assert!(runs[1].style.code && !runs[1].style.pre);
    assert_eq!(runs[2].text, " ");
    assert!(runs[2].style.is_plain());
    assert_eq!(runs[3].text, "c");
    assert!(runs[3].style.pre && !runs[3].style.code);
    assert!(runs[3].style.language.is_none());

    let runs = styled_runs(
        "rust",
        &[entity(
            0,
            4,
            TextEntityKind::PreCode {
                language: "rust".into(),
            },
        )],
    );
    assert_eq!(runs.len(), 1);
    assert!(runs[0].style.pre);
    assert_eq!(runs[0].style.language.as_deref(), Some("rust"));
}

#[test]
fn styled_runs_spoiler_and_link_combine_with_first_link_wins() {
    let text = "secret link";
    let runs = styled_runs(
        text,
        &[
            entity(0, 11, TextEntityKind::Spoiler),
            entity(
                7,
                11,
                TextEntityKind::TextUrl {
                    url: "https://example.com".into(),
                },
            ),
        ],
    );
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "secret ");
    assert!(runs[0].style.spoiler);
    assert!(runs[0].href.is_none());
    assert_eq!(runs[1].text, "link");
    assert!(runs[1].style.spoiler);
    assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
}

#[test]
fn styled_runs_drops_malformed_entities() {
    let text = "ok";
    let runs = styled_runs(
        text,
        &[
            // zero length
            entity(1, 1, TextEntityKind::Bold),
            // past the end
            entity(0, 99, TextEntityKind::Italic),
            // negative-ish (wraps to huge usize start)
            TextEntity {
                utf8_start: usize::MAX - 1,
                utf8_end: usize::MAX,
                kind: TextEntityKind::Underline,
            },
        ],
    );
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].text, "ok");
    assert!(runs[0].style.is_plain());
}

#[test]
fn styled_runs_multibyte_boundaries() {
    // "é" is two UTF-8 bytes; an entity over the whole char styles it,
    // one splitting the bytes is dropped.
    let text = "aé";
    let runs = styled_runs(
        text,
        &[
            entity(1, 3, TextEntityKind::Bold),
            entity(2, 3, TextEntityKind::Italic),
        ],
    );
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[1].text, "é");
    assert!(runs[1].style.bold);
    assert!(!runs[1].style.italic);
}

#[test]
fn styled_runs_empty_text() {
    assert!(styled_runs("", &[entity(0, 0, TextEntityKind::Bold)]).is_empty());
}

#[test]
fn quote_collapse_predicate() {
    assert!(!quote_collapses("one line"));
    assert!(!quote_collapses("one\ntwo\nthree")); // boundary: 3 lines stay
    assert!(quote_collapses("one\ntwo\nthree\nfour"));
    assert_eq!(
        collapsed_quote_len("é\n二\nthree\nfour"),
        "é\n二\nthree".len()
    ); // 4 lines collapse
    let long = (1..=50).map(|i| format!("line {i}")).collect::<Vec<_>>();
    assert!(quote_collapses(&long.join("\n")));
}

#[test]
fn styled_runs_marks_both_blockquote_kinds() {
    for kind in [
        TextEntityKind::BlockQuote,
        TextEntityKind::ExpandableBlockQuote,
    ] {
        let runs = styled_runs("quoted", &[entity(0, 6, kind)]);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].style.quote);
    }
    // Nested styles survive inside a quote.
    let runs = styled_runs(
        "quoted",
        &[
            entity(0, 6, TextEntityKind::BlockQuote),
            entity(0, 6, TextEntityKind::Bold),
        ],
    );
    assert_eq!(runs.len(), 1);
    assert!(runs[0].style.quote && runs[0].style.bold);
}

#[test]
fn rtl_detection_follows_the_first_strong_character() {
    assert!(is_rtl_text("שלום world"));
    assert!(is_rtl_text("12, ❤️ תודה"));
    assert!(!is_rtl_text("hello שלום"));
    assert!(!is_rtl_text("123 !!"));
    assert!(is_rtl_text("مرحبا"));
    assert!(last_line_is_rtl("hello\nתודה דה\n"));
    assert!(!last_line_is_rtl("שלום\nok"));
    // A neutral last line has no direction: the time shares it.
    assert!(!last_line_is_rtl("שלום\n123"));
    assert!(last_line_is_rtl("hello\n12, תודה"));
}

#[test]
fn rtl_detection_skips_neutral_leading_characters() {
    // Digits, punctuation, spaces and emoji carry no direction.
    assert!(is_rtl_text("(123) - שלום"));
    assert!(is_rtl_text("😀 שלום"));
    assert!(is_rtl_text("  \"שלום\""));
    // A paragraph with nothing strong is left to right, empty included.
    assert!(!is_rtl_text(""));
    assert!(!is_rtl_text("123 456"));
    assert!(!is_rtl_text("... ?!"));
    assert!(!is_rtl_text("😀"));
}

#[test]
fn bidi_layout_is_needed_only_for_right_to_left_text() {
    assert!(!has_rtl_text("hello world"));
    assert!(!has_rtl_text("héllo wörld 世界 😀"));
    assert!(has_rtl_text("hello שלום"));
    assert!(has_rtl_text("مرحبا"));
    assert!(!has_rtl_text(""));
}

#[test]
fn rtl_detection_honours_explicit_direction_marks() {
    // A right-to-left mark is a strong right-to-left character.
    assert!(is_rtl_text("\u{200F}123"));
    // Latin first wins over a later Hebrew word.
    assert!(!is_rtl_text("a שלום"));
    // Hebrew first wins over a later Latin word.
    assert!(is_rtl_text("ש abc"));
}

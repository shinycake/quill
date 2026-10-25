//! The Fix result's changes (`diffText`, `schema/td_api.tl:160`) split into
//! runs the box can draw: tdesktop `BuildDiffDisplay` (compose_ai_box.cpp)
//! underlines inserted and replacing text and strikes out deleted and
//! replaced text.
use crate::text::utf16_to_utf8_offset;
use crate::text_split::utf16_len;

/// `diffText`: the text with its changes marked. Deleted text is part of
/// `text`; the replaced old text rides the entity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffText {
    pub text: String,
    pub edits: Vec<DiffEntity>,
}

/// `diffEntity` (`schema/td_api.tl:157`). Offsets and lengths are UTF-16
/// code units, as received; [`diff_segments`] converts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntity {
    pub offset: i32,
    pub length: i32,
    pub kind: DiffKind,
}

/// `DiffEntityType` (`schema/td_api.tl:6389`-`6395`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    Insert,
    Replace { old_text: String },
    Delete,
}

/// One run of [`DiffText::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSegment {
    pub text: String,
    pub kind: DiffSegmentKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSegmentKind {
    Same,
    /// New text (underlined).
    Inserted,
    /// New text replacing `old_text` (old struck out, then new underlined).
    Replaced {
        old_text: String,
    },
    /// Text the fix removed (struck out).
    Deleted,
}

/// The runs of `diff.text` in order. Entities are sorted by offset and
/// clamped to the text like tdesktop does; an entity starting inside an
/// earlier one starts where that one ended, and one whose bounds fall
/// inside a surrogate pair is dropped, so no text is shown twice and no
/// slice can split a character. A replacement with no old text draws as an
/// insertion (tdesktop strikes out only a non-empty old text).
pub fn diff_segments(diff: &DiffText) -> Vec<DiffSegment> {
    let text = diff.text.as_str();
    let size = i64::try_from(utf16_len(text)).unwrap_or(i64::MAX);
    let mut edits: Vec<&DiffEntity> = diff.edits.iter().collect();
    edits.sort_by_key(|edit| edit.offset);
    let mut segments = Vec::new();
    let mut taken_units: i64 = 0;
    let mut taken_bytes: usize = 0;
    for edit in edits {
        let offset = i64::from(edit.offset).clamp(0, size);
        let end = offset
            .saturating_add(i64::from(edit.length).max(0))
            .min(size);
        let start = offset.max(taken_units);
        if end <= start
            && !matches!(&edit.kind, DiffKind::Replace { old_text } if !old_text.is_empty())
        {
            continue;
        }
        let end = end.max(start);
        let (Some(start_byte), Some(end_byte)) = (byte_at(text, start), byte_at(text, end)) else {
            continue;
        };
        if let Some(same) = text.get(taken_bytes..start_byte) {
            push(&mut segments, same, DiffSegmentKind::Same);
        }
        let part = text.get(start_byte..end_byte).unwrap_or_default();
        let kind = match &edit.kind {
            DiffKind::Insert => DiffSegmentKind::Inserted,
            DiffKind::Replace { old_text } if old_text.is_empty() => DiffSegmentKind::Inserted,
            DiffKind::Replace { old_text } => DiffSegmentKind::Replaced {
                old_text: old_text.clone(),
            },
            DiffKind::Delete => DiffSegmentKind::Deleted,
        };
        push(&mut segments, part, kind);
        taken_units = end;
        taken_bytes = end_byte;
    }
    if let Some(rest) = text.get(taken_bytes..) {
        push(&mut segments, rest, DiffSegmentKind::Same);
    }
    segments
}

fn byte_at(text: &str, units: i64) -> Option<usize> {
    utf16_to_utf8_offset(text, i32::try_from(units).ok()?).ok()
}

fn push(segments: &mut Vec<DiffSegment>, text: &str, kind: DiffSegmentKind) {
    let keep = !text.is_empty() || matches!(kind, DiffSegmentKind::Replaced { .. });
    if keep {
        segments.push(DiffSegment {
            text: text.to_string(),
            kind,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(offset: i32, length: i32, kind: DiffKind) -> DiffEntity {
        DiffEntity {
            offset,
            length,
            kind,
        }
    }

    fn same(text: &str) -> DiffSegment {
        DiffSegment {
            text: text.into(),
            kind: DiffSegmentKind::Same,
        }
    }

    fn seg(text: &str, kind: DiffSegmentKind) -> DiffSegment {
        DiffSegment {
            text: text.into(),
            kind,
        }
    }

    #[test]
    fn no_edits_is_one_plain_run() {
        let diff = DiffText {
            text: "all good".into(),
            edits: Vec::new(),
        };
        assert_eq!(diff_segments(&diff), [same("all good")]);
        assert!(diff_segments(&DiffText::default()).is_empty());
    }

    #[test]
    fn insert_marks_the_new_text() {
        let diff = DiffText {
            text: "the big cat".into(),
            edits: vec![entity(4, 4, DiffKind::Insert)],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                same("the "),
                seg("big ", DiffSegmentKind::Inserted),
                same("cat")
            ]
        );
    }

    #[test]
    fn replace_keeps_the_old_text() {
        let diff = DiffText {
            text: "the draft".into(),
            edits: vec![entity(
                0,
                3,
                DiffKind::Replace {
                    old_text: "teh".into(),
                },
            )],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                seg(
                    "the",
                    DiffSegmentKind::Replaced {
                        old_text: "teh".into()
                    }
                ),
                same(" draft")
            ]
        );
    }

    #[test]
    fn replace_without_old_text_is_an_insert() {
        let diff = DiffText {
            text: "ab".into(),
            edits: vec![entity(
                1,
                1,
                DiffKind::Replace {
                    old_text: String::new(),
                },
            )],
        };
        assert_eq!(
            diff_segments(&diff),
            [same("a"), seg("b", DiffSegmentKind::Inserted)]
        );
    }

    #[test]
    fn delete_strikes_out_text_still_in_the_diff() {
        let diff = DiffText {
            text: "very very good".into(),
            edits: vec![entity(5, 5, DiffKind::Delete)],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                same("very "),
                seg("very ", DiffSegmentKind::Deleted),
                same("good")
            ]
        );
    }

    #[test]
    fn utf16_offsets_cover_emoji_and_hebrew() {
        // "😀" is two UTF-16 units and four bytes; each Hebrew letter one
        // unit and two bytes.
        let diff = DiffText {
            text: "😀 שלום עולם".into(),
            edits: vec![
                entity(3, 4, DiffKind::Insert),
                entity(
                    8,
                    4,
                    DiffKind::Replace {
                        old_text: "עלם".into(),
                    },
                ),
            ],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                same("😀 "),
                seg("שלום", DiffSegmentKind::Inserted),
                same(" "),
                seg(
                    "עולם",
                    DiffSegmentKind::Replaced {
                        old_text: "עלם".into()
                    }
                ),
            ]
        );
    }

    #[test]
    fn out_of_range_entities_are_clamped() {
        let diff = DiffText {
            text: "abc".into(),
            edits: vec![
                entity(-5, 1, DiffKind::Insert),
                entity(2, 100, DiffKind::Delete),
                entity(50, 2, DiffKind::Insert),
            ],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                seg("a", DiffSegmentKind::Inserted),
                same("b"),
                seg("c", DiffSegmentKind::Deleted)
            ]
        );
    }

    #[test]
    fn overlapping_entities_never_repeat_text() {
        let diff = DiffText {
            text: "abcdef".into(),
            edits: vec![
                entity(3, 3, DiffKind::Delete),
                entity(1, 3, DiffKind::Insert),
            ],
        };
        assert_eq!(
            diff_segments(&diff),
            [
                same("a"),
                seg("bcd", DiffSegmentKind::Inserted),
                seg("ef", DiffSegmentKind::Deleted)
            ]
        );
    }

    #[test]
    fn entity_inside_a_surrogate_pair_is_dropped() {
        let diff = DiffText {
            text: "😀x".into(),
            edits: vec![entity(1, 2, DiffKind::Insert)],
        };
        assert_eq!(diff_segments(&diff), [same("😀x")]);
    }
}

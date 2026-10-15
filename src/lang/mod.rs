//! Interface language packs (docs/decisions/language-packs.md).
//!
//! Every interface string has a key: `lng_*` keys are Telegram Desktop's
//! (their translations come from Telegram's translation platform through
//! TDLib, `localization_target = "tdesktop"`), `qll_*` keys are Quill's own.
//! English lives in `lang/en/*.strings`; `build.rs` turns it into the
//! [`keys`] constants. Call sites use [`tr`], [`trn`], [`tr_with`] and
//! [`trn_with`]; the chosen pack is a [`Snapshot`] swapped in by
//! [`install`].

mod format;
mod intern;
mod key_table;
pub mod parse;
mod plural;
pub mod prefs;
pub mod search;
mod store;

pub use gpui_shared_string::SharedString;
pub use key_table::{Key, PluralKey, keys};
pub use store::{PackValue, Snapshot, current, install, isolate_thread};

use key_table::{ENGLISH, English};
use parse::Form;
use store::{Value, with_current};

/// The text of `key` in the current language. Pack strings live for the
/// whole run, so the result borrows nothing and fits wherever a literal
/// did (`'static` tables, menu tuples, `impl Into<SharedString>`).
pub fn tr(key: Key) -> &'static str {
    with_current(|snapshot| text_in(snapshot, key))
}

/// `key` with its `{tag}` placeholders replaced (`&[("user", name)]`).
/// A tag the caller does not pass stays as written.
pub fn tr_with(key: Key, tags: &[(&str, &str)]) -> String {
    format::substitute(tr(key), tags)
}

/// The form of `key` for `count`, with `{count}` replaced by the number.
pub fn trn<C: TryInto<i64>>(key: PluralKey, count: C) -> String {
    trn_with(key, count, &[])
}

/// [`trn`] with more `{tag}` placeholders.
pub fn trn_with<C: TryInto<i64>>(key: PluralKey, count: C, tags: &[(&str, &str)]) -> String {
    let count = count.try_into().unwrap_or(i64::MAX);
    with_current(|snapshot| plural_in(snapshot, key, count, tags))
}

pub(crate) fn text_in(snapshot: Option<&Snapshot>, key: Key) -> &'static str {
    if let Some(Value::Text(text)) = snapshot.and_then(|s| s.value(key.0)) {
        return text;
    }
    match ENGLISH.get(usize::from(key.0)) {
        Some(English::Text(text)) => text,
        _ => "",
    }
}

pub(crate) fn plural_in(
    snapshot: Option<&Snapshot>,
    key: PluralKey,
    count: i64,
    tags: &[(&str, &str)],
) -> String {
    let template = plural_template(snapshot, key, count);
    let number = count.to_string();
    let decimal = format::group_digits(count);
    let short = format::short_count(count);
    let mut all: Vec<(&str, &str)> = Vec::with_capacity(tags.len() + 3);
    all.extend_from_slice(tags);
    all.push(("count", &number));
    all.push(("count_decimal", &decimal));
    all.push(("count_short", &short));
    format::substitute(template, &all)
}

fn plural_template(snapshot: Option<&Snapshot>, key: PluralKey, count: i64) -> &'static str {
    let translated = snapshot.and_then(|snapshot| {
        let Some(Value::Plural(forms)) = snapshot.value(key.0) else {
            return None;
        };
        let form = snapshot.plural_form(count);
        forms
            .get(form.index())
            .copied()
            .flatten()
            .or_else(|| forms.get(Form::Other.index()).copied().flatten())
    });
    if let Some(text) = translated {
        return text;
    }
    let Some(English::Plural(forms)) = ENGLISH.get(usize::from(key.0)) else {
        return "";
    };
    let form = plural::english_form(count);
    forms
        .get(form.index())
        .copied()
        .flatten()
        .or_else(|| forms.get(Form::Other.index()).copied().flatten())
        .unwrap_or_default()
}

/// Whether the current pack is written right to left.
pub fn is_rtl() -> bool {
    with_current(|snapshot| snapshot.is_some_and(Snapshot::is_rtl))
}

/// The current pack's id; `"en"` for the built-in English.
pub fn current_pack_id() -> String {
    with_current(|snapshot| snapshot.map_or_else(|| "en".to_string(), |s| s.pack_id().into()))
}

/// The built-in English of `key`, whatever pack is current (tests, logs).
pub fn english(key: Key) -> &'static str {
    match ENGLISH.get(usize::from(key.0)) {
        Some(English::Text(text)) => text,
        _ => "",
    }
}

#[cfg(test)]
mod tests;

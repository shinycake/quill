//! CLDR plural categories for a pack's `plural_code` (ICU4X compiled data,
//! the role of tdesktop's per-language rule table).

use std::sync::Arc;

use icu_locale_core::Locale;
use icu_plurals::{PluralCategory, PluralRulesPreferences};

use super::parse::Form;

/// The plural rules of one language; English when the code is unknown.
#[derive(Debug, Clone)]
pub(crate) struct PluralRules(Option<Arc<icu_plurals::PluralRules>>);

impl PluralRules {
    /// Rules for a pack's `plural_code` (`"ru"`, `"pt-br"`, `"ar"` …).
    pub(crate) fn for_code(code: &str) -> Self {
        let code = code.trim();
        if code.is_empty() || code.eq_ignore_ascii_case("en") {
            return Self(None);
        }
        let rules = Locale::try_from_str(code)
            .or_else(|_| Locale::try_from_str(code.split(['-', '_']).next().unwrap_or(code)))
            .ok()
            .and_then(|locale| {
                icu_plurals::PluralRules::try_new_cardinal(PluralRulesPreferences::from(&locale))
                    .ok()
            });
        Self(rules.map(Arc::new))
    }

    pub(crate) fn form_for(&self, count: i64) -> Form {
        match &self.0 {
            Some(rules) => match rules.category_for(count) {
                PluralCategory::Zero => Form::Zero,
                PluralCategory::One => Form::One,
                PluralCategory::Two => Form::Two,
                PluralCategory::Few => Form::Few,
                PluralCategory::Many => Form::Many,
                PluralCategory::Other => Form::Other,
            },
            None => english_form(count),
        }
    }
}

/// English: "1 item", "0 items", "2 items".
pub(crate) fn english_form(count: i64) -> Form {
    if count == 1 { Form::One } else { Form::Other }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(code: &str, counts: &[i64]) -> Vec<Form> {
        let rules = PluralRules::for_code(code);
        counts.iter().map(|&n| rules.form_for(n)).collect()
    }

    #[test]
    fn english_and_unknown_codes() {
        assert_eq!(
            forms("en", &[0, 1, 2]),
            [Form::Other, Form::One, Form::Other]
        );
        assert_eq!(forms("", &[1, 5]), [Form::One, Form::Other]);
    }

    #[test]
    fn russian_one_few_many() {
        assert_eq!(
            forms("ru", &[1, 2, 5, 21, 22, 11]),
            [
                Form::One,
                Form::Few,
                Form::Many,
                Form::One,
                Form::Few,
                Form::Many
            ]
        );
    }

    #[test]
    fn arabic_six_forms() {
        assert_eq!(
            forms("ar", &[0, 1, 2, 3, 11, 100]),
            [
                Form::Zero,
                Form::One,
                Form::Two,
                Form::Few,
                Form::Many,
                Form::Other
            ]
        );
    }

    #[test]
    fn japanese_other_only_and_french_zero_is_one() {
        assert_eq!(forms("ja", &[0, 1, 2]), [Form::Other; 3]);
        assert_eq!(forms("fr", &[0, 1, 2]), [Form::One, Form::One, Form::Other]);
        assert_eq!(
            forms("pt-br", &[0, 1, 2]),
            [Form::One, Form::One, Form::Other]
        );
    }
}

//! The Language box filter: every word of the query starts a word of the
//! pack's name or native name, ignoring case and diacritics (tdesktop's
//! `PrepareSearchWords` matching).

use icu_normalizer::DecomposingNormalizerBorrowed;

/// Lowercase words with combining marks removed (`"Português"` →
/// `["portugues"]`).
pub fn search_words(text: &str) -> Vec<String> {
    let decomposed = DecomposingNormalizerBorrowed::new_nfd().normalize(text);
    let folded: String = decomposed
        .chars()
        .filter(|c| !matches!(*c as u32, 0x0300..=0x036F))
        .flat_map(char::to_lowercase)
        .collect();
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `query` matches any of `fields`.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let wanted = search_words(query);
    if wanted.is_empty() {
        return true;
    }
    let have: Vec<String> = fields.iter().flat_map(|f| search_words(f)).collect();
    wanted
        .iter()
        .all(|w| have.iter().any(|h| h.starts_with(w.as_str())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_diacritics_and_matches_word_prefixes() {
        assert_eq!(search_words("Português (Brasil)"), ["portugues", "brasil"]);
        assert!(matches(
            "portu",
            &["Portuguese (Brazil)", "Português (Brasil)"]
        ));
        assert!(matches(
            "PORTUGUÊS bra",
            &["Portuguese (Brazil)", "Português (Brasil)"]
        ));
        assert!(matches("рус", &["Russian", "Русский"]));
        assert!(!matches("tuguese", &["Portuguese"]));
        assert!(matches("  ", &["Anything"]));
    }
}

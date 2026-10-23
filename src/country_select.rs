//! The choice made in the country box (tdesktop `Ui::CountrySelectBox` for
//! one country, `Ui::SelectCountriesBox` for a limited list): what is
//! chosen, the limit, and the one-line summary a row shows. The rows come
//! from [`crate::phone::search_countries`].
#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, trn};
use crate::phone::Country;

/// tdesktop `AppConfig::pollCountriesMax` fallback, used until TDLib's
/// `poll_country_count_max` arrives.
pub const POLL_COUNTRIES_FALLBACK: usize = 12;

/// How many countries the box takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountryMode {
    /// One country; choosing a row ends the box.
    Single,
    /// Up to `max` countries, kept until Save.
    Multi { max: usize },
}

/// What a click on a row did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountryClick {
    /// Single mode: this country is the answer.
    Picked(String),
    Added,
    Removed,
    /// Multi mode and already at the limit; nothing changed.
    LimitReached(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountrySelection {
    mode: CountryMode,
    chosen: Vec<String>,
}

impl CountrySelection {
    /// `chosen` is cut to the limit in multi mode and to one in single mode.
    pub fn new(mode: CountryMode, chosen: &[String]) -> Self {
        let keep = match mode {
            CountryMode::Single => 1,
            CountryMode::Multi { max } => max,
        };
        let mut list: Vec<String> = Vec::new();
        for iso in chosen {
            if list.len() < keep && !list.contains(iso) {
                list.push(iso.clone());
            }
        }
        Self { mode, chosen: list }
    }

    pub fn mode(&self) -> CountryMode {
        self.mode
    }

    pub fn chosen(&self) -> &[String] {
        &self.chosen
    }

    pub fn contains(&self, iso: &str) -> bool {
        self.chosen.iter().any(|code| code == iso)
    }

    pub fn click(&mut self, iso: &str) -> CountryClick {
        match self.mode {
            CountryMode::Single => {
                self.chosen = vec![iso.to_string()];
                CountryClick::Picked(iso.to_string())
            }
            CountryMode::Multi { max } => {
                if let Some(index) = self.chosen.iter().position(|code| code == iso) {
                    self.chosen.remove(index);
                    CountryClick::Removed
                } else if self.chosen.len() >= max {
                    CountryClick::LimitReached(max)
                } else {
                    self.chosen.push(iso.to_string());
                    CountryClick::Added
                }
            }
        }
    }

    pub fn remove(&mut self, iso: &str) {
        self.chosen.retain(|code| code != iso);
    }
}

/// The name TDLib gives a country, or the code when the list has no such
/// row (or has not arrived).
pub fn country_name(iso: &str, countries: &[Country]) -> String {
    countries
        .iter()
        .find(|country| country.iso.eq_ignore_ascii_case(iso))
        .map(|country| country.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| iso.to_string())
}

/// What the "Allowed Countries" row shows: nothing, the name, or "N countries"
/// (tdesktop `lng_polls_create_countries_count`).
pub fn summary(chosen: &[String], countries: &[Country]) -> String {
    match chosen {
        [] => String::new(),
        [one] => country_name(one, countries),
        many => trn(keys::POLLS_CREATE_COUNTRIES_COUNT, many.len()),
    }
}

/// The toast when the limit stops a click
/// (tdesktop `lng_polls_create_countries_limit`).
pub fn limit_text(max: usize) -> String {
    trn(keys::POLLS_CREATE_COUNTRIES_LIMIT, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(list: &[&str]) -> Vec<String> {
        list.iter().map(|code| code.to_string()).collect()
    }

    fn country(iso: &str, name: &str) -> Country {
        Country {
            iso: iso.into(),
            name: name.into(),
            english_name: name.into(),
            flag: String::new(),
            calling_codes: vec!["1".into()],
            hidden: false,
        }
    }

    #[test]
    fn single_mode_picks_one_country() {
        let mut selection = CountrySelection::new(CountryMode::Single, &codes(&["US"]));
        assert_eq!(selection.click("FR"), CountryClick::Picked("FR".into()));
        assert_eq!(selection.chosen(), ["FR"]);
    }

    #[test]
    fn multi_mode_toggles_and_stops_at_the_limit() {
        let mut selection = CountrySelection::new(CountryMode::Multi { max: 2 }, &[]);
        assert_eq!(selection.click("US"), CountryClick::Added);
        assert_eq!(selection.click("GB"), CountryClick::Added);
        assert_eq!(selection.click("FR"), CountryClick::LimitReached(2));
        assert_eq!(selection.chosen(), ["US", "GB"]);
        assert_eq!(selection.click("US"), CountryClick::Removed);
        assert_eq!(selection.click("FR"), CountryClick::Added);
        assert_eq!(selection.chosen(), ["GB", "FR"]);
    }

    #[test]
    fn a_long_start_list_is_cut_to_the_limit_without_repeats() {
        let selection = CountrySelection::new(
            CountryMode::Multi { max: 2 },
            &codes(&["US", "US", "GB", "FR"]),
        );
        assert_eq!(selection.chosen(), ["US", "GB"]);
    }

    #[test]
    fn remove_drops_a_chip() {
        let mut selection =
            CountrySelection::new(CountryMode::Multi { max: 5 }, &codes(&["US", "GB"]));
        selection.remove("US");
        assert!(!selection.contains("US"));
        assert!(selection.contains("GB"));
    }

    #[test]
    fn names_fall_back_to_the_code() {
        let list = vec![country("US", "United States")];
        assert_eq!(country_name("us", &list), "United States");
        assert_eq!(country_name("ZZ", &list), "ZZ");
        assert_eq!(summary(&[], &list), "");
        assert_eq!(summary(&codes(&["US"]), &list), "United States");
        assert_eq!(summary(&codes(&["US", "GB"]), &list), "2 countries");
        assert_eq!(limit_text(12), "You can choose up to 12 countries.");
    }
}

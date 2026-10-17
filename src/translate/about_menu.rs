//! The context menu of a profile's about text. tdesktop
//! (`info_profile_actions.cpp`, `setupAboutContextMenu`) lets the label fill
//! its usual menu (Copy Selected Text over a selection, Copy Text
//! otherwise) and then adds Translate Selected Text or Translate, unless
//! `SkipTranslate` holds for the text the item would translate.

/// The copy item of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AboutCopy {
    /// Copy Selected Text.
    Selected(String),
    /// Copy Text: the whole about text.
    Text(String),
}

/// The translate item of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutTranslate {
    /// Translate Selected Text (`true`) or Translate (`false`).
    pub selected: bool,
    /// What the box translates.
    pub text: String,
}

/// Which items the about text's menu shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutMenu {
    pub copy: Option<AboutCopy>,
    pub translate: Option<AboutTranslate>,
}

/// The menu for `full` with `selection` selected (if any). `skip` is
/// `SkipTranslate`: it covers the translate button setting, letterless text
/// and the "Do Not Translate" languages.
pub fn about_menu(full: &str, selection: Option<&str>, skip: impl Fn(&str) -> bool) -> AboutMenu {
    let selection = selection.filter(|selected| !selected.is_empty());
    let copy = match selection {
        Some(selected) => Some(AboutCopy::Selected(selected.to_string())),
        None if full.is_empty() => None,
        None => Some(AboutCopy::Text(full.to_string())),
    };
    let text = selection.unwrap_or(full);
    let translate = (!skip(text)).then(|| AboutTranslate {
        selected: selection.is_some(),
        text: text.to_string(),
    });
    AboutMenu { copy, translate }
}

#[cfg(test)]
mod tests {
    use super::{AboutCopy, AboutMenu, AboutTranslate, about_menu};
    use crate::translate::{TranslatePrefs, skip_translate};

    fn never(_: &str) -> bool {
        false
    }

    #[test]
    fn whole_text_offers_copy_text_and_translate() {
        assert_eq!(
            about_menu("Bonjour tout le monde", None, never),
            AboutMenu {
                copy: Some(AboutCopy::Text("Bonjour tout le monde".into())),
                translate: Some(AboutTranslate {
                    selected: false,
                    text: "Bonjour tout le monde".into()
                }),
            }
        );
    }

    #[test]
    fn a_selection_offers_the_selected_variants() {
        let menu = about_menu("Bonjour tout le monde", Some("tout"), never);
        assert_eq!(menu.copy, Some(AboutCopy::Selected("tout".into())));
        assert_eq!(
            menu.translate,
            Some(AboutTranslate {
                selected: true,
                text: "tout".into()
            })
        );
    }

    #[test]
    fn skip_decides_on_the_selection_not_the_whole_text() {
        let skip = |text: &str| text == "tout";
        assert!(
            about_menu("Bonjour tout", Some("tout"), skip)
                .translate
                .is_none()
        );
        let menu = about_menu("Bonjour tout", Some("Bonjour"), skip);
        assert!(menu.translate.is_some());
    }

    #[test]
    fn skipped_text_still_copies() {
        let menu = about_menu("Hello", None, |_| true);
        assert_eq!(menu.copy, Some(AboutCopy::Text("Hello".into())));
        assert!(menu.translate.is_none());
    }

    #[test]
    fn empty_text_has_no_items() {
        let menu = about_menu("", None, |text| text.is_empty());
        assert_eq!(
            menu,
            AboutMenu {
                copy: None,
                translate: None
            }
        );
    }

    #[test]
    fn an_empty_selection_counts_as_none() {
        let menu = about_menu("Hello there", Some(""), never);
        assert_eq!(menu.copy, Some(AboutCopy::Text("Hello there".into())));
    }

    #[test]
    fn real_skip_follows_the_button_setting_and_languages() {
        let on = TranslatePrefs::default();
        let off = TranslatePrefs {
            show_button: false,
            ..TranslatePrefs::default()
        };
        let french = "Bonjour à tous, bienvenue sur ma page personnelle";
        assert!(
            about_menu(french, None, |t| skip_translate(t, &on, "en"))
                .translate
                .is_some()
        );
        assert!(
            about_menu(french, None, |t| skip_translate(t, &off, "en"))
                .translate
                .is_none()
        );
        assert!(
            about_menu("12345 :) 678", None, |t| skip_translate(t, &on, "en"))
                .translate
                .is_none()
        );
        let english = "Hello, how are you? I think this is the best one for you.";
        assert!(
            about_menu(english, None, |t| skip_translate(t, &on, "en"))
                .translate
                .is_none()
        );
    }
}

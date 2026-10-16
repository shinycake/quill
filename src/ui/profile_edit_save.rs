//! The edit-profile form's single Save: which fields changed, the bio
//! counter, and the native photo picker.
//!
//! tdesktop (`settings/settings_information.cpp`) keeps one form: the
//! name and bio fields are saved together (only what changed), the photo
//! is chosen with the file dialog and uploaded at once.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;

use gpui_kit::*;
use quill::lang::{keys, tr};
use std::path::Path;

/// TDLib's compiled default for `bio_length_max` (Premium raises it).
pub(crate) const DEFAULT_BIO_LENGTH_MAX: usize = 70;

/// The name and bio fields of the form, normalised the way they are sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProfileFields {
    pub(crate) first: String,
    pub(crate) last: String,
    /// Newlines collapsed (the schema allows no line feeds), trimmed.
    pub(crate) bio: String,
}

/// One request the Save button sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProfileEdit {
    /// `setName`; TDLib takes both parts, so a changed last name sends the
    /// unchanged first name along.
    Name {
        first: String,
        last: String,
    },
    SetBio(String),
}

impl ProfileFields {
    pub(crate) fn new(first: &str, last: &str, bio: &str) -> Self {
        Self {
            first: first.trim().to_string(),
            last: last.trim().to_string(),
            bio: bio.replace(['\n', '\r'], " ").trim().to_string(),
        }
    }

    /// The requests that bring `self` (the opening values) to `now`.
    pub(crate) fn edits_to(&self, now: &Self) -> Vec<ProfileEdit> {
        let mut edits = Vec::new();
        if self.first != now.first || self.last != now.last {
            edits.push(ProfileEdit::Name {
                first: now.first.clone(),
                last: now.last.clone(),
            });
        }
        if self.bio != now.bio {
            edits.push(ProfileEdit::SetBio(now.bio.clone()));
        }
        edits
    }

    /// Characters left in the bio (negative when over the limit).
    pub(crate) fn bio_remaining(&self, limit: usize) -> i64 {
        let used = i64::try_from(self.bio.chars().count()).unwrap_or(i64::MAX);
        i64::try_from(limit)
            .unwrap_or(i64::MAX)
            .saturating_sub(used)
    }
}

impl QuillApp {
    /// The bio limit: TDLib's `bio_length_max` option.
    pub(super) fn profile_bio_limit(&self) -> usize {
        self.session()
            .map_or(DEFAULT_BIO_LENGTH_MAX, |s| s.users_state.bio_length_max)
    }

    /// Whether Save has something to send (and may send it).
    pub(super) fn profile_form_can_save(&self, cx: &App) -> bool {
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return false;
        };
        let now = dialog.current_fields(cx);
        !dialog.initial.edits_to(&now).is_empty()
            && !now.first.is_empty()
            && now.bio_remaining(self.profile_bio_limit()) >= 0
    }

    /// The Save button: send only the changed fields, then close.
    pub(super) fn save_profile_form(&mut self, cx: &mut Context<Self>) {
        if !self.profile_form_can_save(cx) {
            return;
        }
        let Some(dialog) = &self.dialogs.edit_profile_dialog else {
            return;
        };
        let edits = dialog.initial.edits_to(&dialog.current_fields(cx));
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = tr(keys::Q_PEOPLE_DEMO_MODE_PROFILE_EDITS_NEED).into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        for edit in edits {
            let sent = match &edit {
                ProfileEdit::Name { first, last } => live.driver.set_name(first, last),
                ProfileEdit::SetBio(bio) => live.driver.set_bio(bio),
            };
            if let Err(err) = sent {
                self.connection.status_note = format!("save profile failed: {err:?}");
                cx.notify();
                return;
            }
        }
        self.connection.status_note = tr(keys::Q_PEOPLE_NAME_UPDATE_REQUESTED).into();
        self.close_edit_profile_dialog(cx);
    }

    /// "Set Photo": the native file dialog, then `setProfilePhoto` (the
    /// photo is not part of Save, as in tdesktop).
    pub(super) fn choose_profile_photo(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::PROFILE_SET_PHOTO_BUTTON).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first().cloned()
            {
                let _ = this.update(cx, |this, cx| this.submit_profile_photo(&path, cx));
            }
        })
        .detach();
    }

    /// `setProfilePhoto` from a local path (`inputChatPhotoStatic`); sets
    /// the main profile photo (`is_public=false`).
    pub(super) fn submit_profile_photo(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(path) = path.to_str() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = tr(keys::Q_PEOPLE_DEMO_MODE_PROFILE_EDITS_NEED).into();
            cx.notify();
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match live.driver.set_profile_photo(path) {
            Ok(_) => self.connection.status_note = tr(keys::Q_PEOPLE_PHOTO_UPDATE_REQUESTED).into(),
            Err(err) => self.connection.status_note = format!("set photo failed: {err:?}"),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{ProfileEdit, ProfileFields};

    fn fields(first: &str, last: &str, bio: &str) -> ProfileFields {
        ProfileFields::new(first, last, bio)
    }

    #[test]
    fn nothing_changed_sends_nothing() {
        let open = fields("Demo", "Viewer", "Hello");
        assert!(
            open.edits_to(&fields(" Demo ", "Viewer", "Hello\n"))
                .is_empty()
        );
    }

    #[test]
    fn only_changed_fields_are_sent() {
        let open = fields("Demo", "Viewer", "Hello");
        assert_eq!(
            open.edits_to(&fields("Demo", "Viewer", "Bye")),
            vec![ProfileEdit::SetBio("Bye".into())]
        );
        assert_eq!(
            open.edits_to(&fields("Demo", "Other", "Hello")),
            vec![ProfileEdit::Name {
                first: "Demo".into(),
                last: "Other".into()
            }]
        );
        assert_eq!(open.edits_to(&fields("A", "B", "C")).len(), 2);
    }

    #[test]
    fn bio_newlines_collapse_and_count_remaining() {
        let f = fields("A", "", "ab\ncd");
        assert_eq!(f.bio, "ab cd");
        assert_eq!(f.bio_remaining(70), 65);
        assert_eq!(f.bio_remaining(3), -2);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;

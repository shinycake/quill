//! A language pack opened from `t.me/setlanguage/<id>`: what
//! `getLanguagePackInfo` said. Quill never switches language from a link.
use crate::state::*;
use crate::telegram::envelope::LanguagePackInfoData;

/// The pack a link names and the progress of looking it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageLinkLookup {
    pub id: String,
    /// `None` while loading or after a failed lookup.
    pub info: Option<LanguagePackInfoData>,
    pub loading: bool,
    pub error: Option<String>,
}

impl LanguageLinkLookup {
    pub fn new(id: String) -> Self {
        Self {
            id,
            info: None,
            loading: true,
            error: None,
        }
    }
}

impl Session {
    /// `languagePackInfo` for our own `getLanguagePackInfo`.
    pub(crate) fn apply_language_pack_info(
        &mut self,
        info: LanguagePackInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLanguagePackInfo)
            && let Some(lookup) = self.settings.language_link.as_mut()
        {
            lookup.loading = false;
            lookup.error = None;
            lookup.info = Some(info);
        }
    }

    pub(crate) fn apply_language_pack_error(&mut self, pending: Option<&PendingRequest>) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLanguagePackInfo)
            && let Some(lookup) = self.settings.language_link.as_mut()
        {
            lookup.loading = false;
            lookup.info = None;
            lookup.error = Some("This language link is not valid.".into());
        }
    }
}

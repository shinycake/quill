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
    /// The `getLanguagePackInfo` this box waits for; another box's answer
    /// settles nothing here.
    pub(crate) request: Option<RequestId>,
}

impl LanguageLinkLookup {
    pub fn new(id: String) -> Self {
        Self {
            id,
            info: None,
            loading: true,
            error: None,
            request: None,
        }
    }

    /// The box waits for `request` to answer.
    pub(crate) fn waiting_for(mut self, request: RequestId) -> Self {
        self.request = Some(request);
        self
    }

    /// `pending` is this box's own lookup: settle it.
    fn take_answer(&mut self, pending: Option<&PendingRequest>) -> bool {
        let ours = pending.is_some_and(|p| {
            p.purpose == RequestPurpose::GetLanguagePackInfo && Some(p.id) == self.request
        });
        if ours {
            self.request = None;
            self.loading = false;
        }
        ours
    }
}

impl Session {
    /// `languagePackInfo` for our own `getLanguagePackInfo`.
    pub(crate) fn apply_language_pack_info(
        &mut self,
        info: LanguagePackInfoData,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(lookup) = self.settings.language_link.as_mut()
            && lookup.take_answer(pending)
        {
            lookup.error = None;
            lookup.info = Some(info);
        }
    }

    pub(crate) fn apply_language_pack_error(&mut self, pending: Option<&PendingRequest>) {
        if let Some(lookup) = self.settings.language_link.as_mut()
            && lookup.take_answer(pending)
        {
            lookup.info = None;
            lookup.error = Some("This language link is not valid.".into());
        }
    }
}

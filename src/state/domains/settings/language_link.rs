//! A language pack opened from `t.me/setlanguage/<id>`: what
//! `getLanguagePackInfo` said. Quill never switches language from a link.
use crate::state::*;
use crate::telegram::envelope::LanguagePackInfoData;

/// Where the lookup of a linked pack stands. It leaves by the answer, the
/// error, or the box closing (the lookup is dropped with it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageLinkStage {
    /// `getLanguagePackInfo` is out as this request; another box's answer
    /// settles nothing here.
    Looking(RequestId),
    Known(LanguagePackInfoData),
    Failed(String),
}

/// The pack a link names and the progress of looking it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageLinkLookup {
    pub id: String,
    pub stage: LanguageLinkStage,
}

impl LanguageLinkLookup {
    /// A box whose lookup went out as `request`.
    pub fn looking_up(id: String, request: RequestId) -> Self {
        Self {
            id,
            stage: LanguageLinkStage::Looking(request),
        }
    }

    pub fn loading(&self) -> bool {
        matches!(self.stage, LanguageLinkStage::Looking(_))
    }

    /// The pack, once the lookup answered.
    pub fn info(&self) -> Option<&LanguagePackInfoData> {
        match &self.stage {
            LanguageLinkStage::Known(info) => Some(info),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            LanguageLinkStage::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// `pending` is this box's own lookup.
    fn is_ours(&self, pending: Option<&PendingRequest>) -> bool {
        matches!(self.stage, LanguageLinkStage::Looking(request)
            if pending.is_some_and(|p| {
                p.purpose == RequestPurpose::GetLanguagePackInfo && p.id == request
            }))
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
            && lookup.is_ours(pending)
        {
            lookup.stage = LanguageLinkStage::Known(info);
        }
    }

    pub(crate) fn apply_language_pack_error(&mut self, pending: Option<&PendingRequest>) {
        if let Some(lookup) = self.settings.language_link.as_mut()
            && lookup.is_ours(pending)
        {
            lookup.stage = LanguageLinkStage::Failed("This language link is not valid.".into());
        }
    }
}

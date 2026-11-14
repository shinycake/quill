//! Settings → Language: the beta packs. Telegram lists only its official
//! packs for Telegram Desktop's strings (`localizationTargetInfo`), so the
//! community packs it marks as beta (shared as `t.me/setlanguage/<code>-beta`,
//! answered with `<code>-raw` ids) are asked for one by one and listed
//! under their own heading (docs/decisions/language-packs.md).
use crate::state::fetch::Fetch;
use crate::state::*;
use crate::telegram::envelope::LanguagePackInfoData;

/// The beta packs Quill asks about, by link id. Checked 2026-11-14: each
/// was at least 20% translated for Telegram Desktop. Which ones show is
/// decided by how far each is translated now ([`BetaPack::listed`]).
pub const BETA_LANGUAGE_PACKS: &[&str] = &[
    "ar-beta",
    "eo-beta",
    "eu-beta",
    "fa-beta",
    "fil-beta",
    "ga-beta",
    "gl-beta",
    "he-beta",
    "hu-beta",
    "ja-beta",
    "pt-pt-beta",
    "sk-beta",
    "sv-beta",
    "sw-beta",
    "ta-beta",
    "uz-beta",
    "vi-beta",
];

/// A beta pack is listed once at least this share of its strings is
/// translated; below it most of the interface would stay in English.
const LISTED_SHARE_PERCENT: i64 = 50;

/// One beta pack and its details from `getLanguagePackInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct BetaPack {
    pub link_id: &'static str,
    pub info: Fetch<LanguagePackInfoData>,
}

impl BetaPack {
    /// The pack's details when it is a beta pack translated far enough to
    /// list.
    pub fn listed(&self) -> Option<&LanguagePackInfoData> {
        self.info
            .data()
            .filter(|info| info.is_beta && translated_percent(info) >= LISTED_SHARE_PERCENT)
    }
}

/// How much of the pack is translated, rounded down (0 when the pack has
/// no strings).
pub fn translated_percent(info: &LanguagePackInfoData) -> i64 {
    let total = info.total_string_count.max(0);
    let done = info.translated_string_count.clamp(0, total);
    done.saturating_mul(100).checked_div(total).unwrap_or(0)
}

impl LanguageState {
    /// The beta packs, each not yet asked about.
    pub fn betas_mut(&mut self) -> &mut Vec<BetaPack> {
        if self.betas.is_empty() {
            self.betas = BETA_LANGUAGE_PACKS
                .iter()
                .map(|&link_id| BetaPack {
                    link_id,
                    info: Fetch::Idle,
                })
                .collect();
        }
        &mut self.betas
    }

    /// The beta packs to list, in the order of [`BETA_LANGUAGE_PACKS`].
    pub fn listed_betas(&self) -> Vec<LanguagePackInfoData> {
        self.betas
            .iter()
            .filter_map(BetaPack::listed)
            .cloned()
            .collect()
    }
}

impl Session {
    /// `languagePackInfo` of a beta pack: taken by the pack that asked.
    pub(crate) fn apply_beta_language_info(
        &mut self,
        info: LanguagePackInfoData,
        request: Option<RequestId>,
    ) {
        if let Some(beta) = self
            .settings
            .language
            .betas
            .iter_mut()
            .find(|beta| request.is_some_and(|r| beta.info.waits_for(r)))
        {
            beta.info.answered(request, info);
        }
    }

    /// A beta pack's lookup was refused: that pack is not listed.
    pub(crate) fn apply_beta_language_error(&mut self, request: Option<RequestId>, reason: String) {
        for beta in &mut self.settings.language.betas {
            if beta.info.failed(request, reason.clone(), true) {
                break;
            }
        }
    }
}

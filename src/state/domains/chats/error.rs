//! Failed requests for chat-level look and actions: backgrounds, themes, deep links, action bar.
use crate::state::*;

impl Session {
    /// Reacts to a failed chats request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_chats_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(
                RequestPurpose::GetInstalledBackgrounds
                | RequestPurpose::SetDefaultBackground
                | RequestPurpose::DeleteDefaultBackground
                | RequestPurpose::RemoveInstalledBackground
                | RequestPurpose::SetDefaultBackgroundLocal
                | RequestPurpose::SearchBackground
                | RequestPurpose::SetChatBackground
                | RequestPurpose::DeleteChatBackground
                | RequestPurpose::SetChatTheme,
            ) => {
                self.chats_state.background_error = Some(error_reason(err));
            }
            // Slice CL3: refused report / block surfaces in the
            // status note — never shown as success.
            Some(RequestPurpose::ReportChat) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not report the chat (error {})", err.code));
            }
            Some(RequestPurpose::RemoveChatActionBar) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not hide the bar (error {})", err.code));
            }
            Some(RequestPurpose::CreatePrivateChat) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not open Saved Messages (error {})",
                    err.code
                ));
            }
            // `parity:platform-deep-links`: a failed deep-link request
            // surfaces TDLib's error as a dialog; stale failures (a newer
            // flow is already in flight) are ignored via the generation
            // guard.
            Some(
                RequestPurpose::Chats(ChatsPurpose::DeepLinkInfo { generation })
                | RequestPurpose::Chats(ChatsPurpose::DeepLinkInternalType { generation })
                | RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve { generation })
                | RequestPurpose::Chats(ChatsPurpose::DeepLinkJoin { generation })
                | RequestPurpose::Chats(ChatsPurpose::DeepLinkCheckInvite { generation }),
            ) => {
                let stale = !matches!(
                    &self.chats_state.deep_link,
                    Some(
                        DeepLinkState::ResolvingInfo {
                            generation: slot
                        }
                        | DeepLinkState::ResolvingChat {
                            generation: slot,
                            ..
                        }
                    ) if *slot == generation
                );
                if !stale {
                    let text = deep_link_error_text(self.chats_state.deep_link.as_ref(), err.code);
                    self.chats_state.deep_link = Some(DeepLinkState::ShowText(text));
                }
            }
            _ => {}
        }
    }
}

/// tdesktop's wording for a failed link (`lng_username_not_found`,
/// `lng_group_invite_bad_link`); other failures keep the error code.
pub(crate) fn deep_link_error_text(flow: Option<&DeepLinkState>, code: i32) -> String {
    let not_found = matches!(code, 400 | 404);
    match flow {
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::OpenUsername { domain, .. },
            ..
        }) if not_found => format!("The username \"{domain}\" is not occupied by anyone."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::OpenPublicChatDraft { domain, .. },
            ..
        }) if not_found => format!("The username \"{domain}\" is not occupied by anyone."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::ShareGame { domain, .. } | DeepLinkAction::AddBot { domain, .. },
            ..
        }) if not_found => format!("The username \"{domain}\" is not occupied by anyone."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::UserPhone { phone, .. },
            ..
        }) if not_found => format!("The phone number +{phone} is not on Telegram yet."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::StickerSet { .. },
            ..
        }) if not_found => "This sticker set doesn't exist.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::MessageLink { .. },
            ..
        }) if not_found => "This message link is broken or the chat is not available.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::BoostLink { .. },
            ..
        }) if not_found => "This boost link is broken.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::JoinInvite { .. },
            ..
        }) if not_found => "This invite link is broken or has expired.".to_string(),
        Some(DeepLinkState::ResolvingInfo { .. }) if not_found => {
            "This link isn't supported by Quill.".to_string()
        }
        _ => format!("Couldn't open the link (error {code})."),
    }
}

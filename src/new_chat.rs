//! New Group and New Channel (tdesktop `GroupInfoBox`, `SetupChannelBox`
//! and `AddParticipantsBoxController::Start` in `boxes/add_contact_box.cpp`
//! and `boxes/peers/add_participants_box.cpp`).
//!
//! A group takes its photo and name, then its members, and is created
//! with them (`createNewBasicGroupChat`). A channel takes its photo, name
//! and description and is created at once (`createNewSupergroupChat`);
//! then it is made public or private, then subscribers are added, both
//! skippable. The photo is set after creation (`setChatPhoto`), as
//! tdesktop uploads it after `messages.createChat` / `channels.createChannel`.
#![deny(clippy::indexing_slicing)]

use crate::group_info_edit::{DESCRIPTION_MAX, TITLE_MAX};
use crate::ids::{ChatId, RequestId};
use crate::lang::{Key, keys};
use crate::premium_hub::group_digits;

/// What is being made (tdesktop `GroupInfoBox::Type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewChatKind {
    Group,
    /// A supergroup made straight away (Quill's "New supergroup").
    Megagroup,
    Channel,
}

impl NewChatKind {
    /// Created with `createNewSupergroupChat`, then the type and
    /// subscribers steps.
    pub fn is_supergroup(self) -> bool {
        !matches!(self, Self::Group)
    }

    /// Only a basic group skips the description field.
    pub fn has_description(self) -> bool {
        self.is_supergroup()
    }

    /// The top bar "Auto-Delete" menu shows for a group only.
    pub fn has_auto_delete_menu(self) -> bool {
        matches!(self, Self::Group)
    }

    /// The first box's confirm button: a group goes on to its members.
    pub fn info_confirm(self) -> Key {
        match self {
            Self::Group => keys::CREATE_GROUP_NEXT,
            _ => keys::CREATE_GROUP_CREATE,
        }
    }

    /// The name field's label and placeholder.
    pub fn name_label(self) -> Key {
        match self {
            Self::Channel => keys::DLG_NEW_CHANNEL_NAME,
            _ => keys::DLG_NEW_GROUP_NAME,
        }
    }

    /// The auto-delete period the request carries: tdesktop sends the
    /// group's chosen (or the account's default) period with a group and
    /// a supergroup, and none with a channel.
    pub fn auto_delete(self, chosen: i32) -> i32 {
        match self {
            Self::Channel => 0,
            _ => chosen.max(0),
        }
    }
}

/// Where the person has taken the flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewChatStage {
    /// Photo, name and (not for a group) description.
    Info,
    /// The group's auto-delete period picker (tdesktop `TTLBox`).
    AutoDelete,
    /// The group's members picker; Create sends.
    Members,
    /// The create request is out; `back` is where a refusal returns.
    Creating {
        request: RequestId,
        back: NewChatBack,
    },
    /// The supergroup or channel exists: public or private. `saving`
    /// holds the requests of the type's Save.
    Type {
        chat: ChatId,
        saving: Option<Vec<RequestId>>,
    },
    /// Adding subscribers (or members) to the new supergroup or channel.
    Subscribers { chat: ChatId },
}

/// The step a refused create returns to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewChatBack {
    Info,
    Members,
}

/// What the dialog shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewChatStep {
    Info,
    AutoDelete,
    Members,
    Type(ChatId),
    Subscribers(ChatId),
    /// The group was made; the dialog is gone and the chat opens.
    Done(ChatId),
}

/// Why TDLib refused the create (tdesktop `GroupInfoBox` fail handlers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateFailure {
    /// `NO_CHAT_TITLE`: the name field shows its error.
    NoTitle,
    /// `USERS_TOO_FEW`: `lng_cant_invite_privacy`.
    UsersTooFew,
    /// `PEER_FLOOD`: `lng_cant_invite_not_contact`.
    PeerFlood,
    /// `USER_RESTRICTED`: `lng_cant_do_this`.
    Restricted,
    /// `CHANNELS_TOO_MUCH`: the communities limit box opens instead.
    TooManyChannels,
    /// Anything else: the reason, said plainly.
    Other,
}

impl CreateFailure {
    pub fn from_server_type(server_type: Option<&str>) -> Self {
        match server_type {
            Some("NO_CHAT_TITLE") => Self::NoTitle,
            Some("USERS_TOO_FEW") => Self::UsersTooFew,
            Some("PEER_FLOOD") => Self::PeerFlood,
            Some("USER_RESTRICTED") => Self::Restricted,
            Some("CHANNELS_TOO_MUCH") => Self::TooManyChannels,
            _ => Self::Other,
        }
    }

    /// tdesktop's message for the refusal; `None` when the name field or
    /// the limit box says it instead.
    pub fn message(self) -> Option<Key> {
        match self {
            Self::UsersTooFew => Some(keys::CANT_INVITE_PRIVACY),
            Self::PeerFlood => Some(keys::CANT_INVITE_NOT_CONTACT),
            Self::Restricted => Some(keys::CANT_DO_THIS),
            Self::NoTitle | Self::TooManyChannels | Self::Other => None,
        }
    }
}

/// How the create request ended, by its request id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreationOutcome {
    Created(ChatId),
    Failed {
        failure: CreateFailure,
        reason: String,
    },
}

/// What the session knows about the stage's requests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageFacts {
    /// The create request is still out.
    pub create_pending: bool,
    pub outcome: Option<CreationOutcome>,
    /// A request of the type's Save is still out.
    pub type_pending: bool,
    /// TDLib refused the link of the type's Save.
    pub type_failed: bool,
}

/// What the dialog shows for `stage`, given the session's facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewChatView {
    pub step: NewChatStep,
    /// A request of this step is out: its confirm button waits.
    pub busy: bool,
    /// The refused create, shown on the step it returned to.
    pub failure: Option<(CreateFailure, String)>,
}

pub fn view(kind: NewChatKind, stage: &NewChatStage, facts: &StageFacts) -> NewChatView {
    let plain = |step| NewChatView {
        step,
        busy: false,
        failure: None,
    };
    match stage {
        NewChatStage::Info => plain(NewChatStep::Info),
        NewChatStage::AutoDelete => plain(NewChatStep::AutoDelete),
        NewChatStage::Members => plain(NewChatStep::Members),
        NewChatStage::Creating { back, .. } => {
            let back = match back {
                NewChatBack::Info => NewChatStep::Info,
                NewChatBack::Members => NewChatStep::Members,
            };
            match &facts.outcome {
                Some(CreationOutcome::Created(chat)) if kind.is_supergroup() => {
                    plain(NewChatStep::Type(*chat))
                }
                Some(CreationOutcome::Created(chat)) => plain(NewChatStep::Done(*chat)),
                Some(CreationOutcome::Failed { failure, reason }) => NewChatView {
                    step: back,
                    busy: false,
                    failure: Some((*failure, reason.clone())),
                },
                None => NewChatView {
                    step: back,
                    busy: facts.create_pending,
                    failure: None,
                },
            }
        }
        NewChatStage::Type { chat, saving: None } => plain(NewChatStep::Type(*chat)),
        NewChatStage::Type {
            chat,
            saving: Some(_),
        } => {
            if facts.type_pending {
                NewChatView {
                    step: NewChatStep::Type(*chat),
                    busy: true,
                    failure: None,
                }
            } else if facts.type_failed {
                plain(NewChatStep::Type(*chat))
            } else {
                plain(NewChatStep::Subscribers(*chat))
            }
        }
        NewChatStage::Subscribers { chat } => plain(NewChatStep::Subscribers(*chat)),
    }
}

/// One create request, as TDLib takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewChatRequest {
    pub kind: NewChatKind,
    pub title: String,
    pub description: String,
    /// The group's members, without the viewer.
    pub user_ids: Vec<i64>,
    /// Seconds; 0 is off.
    pub auto_delete: i32,
}

/// Why the confirm button sends nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewChatInvalid {
    /// The name field shows its error (tdesktop `_title->showError()`).
    EmptyTitle,
    TitleTooLong,
    DescriptionTooLong,
}

/// The request for the fields as typed, trimmed as tdesktop's
/// `PrepareForSending` sends them.
pub fn plan_new_chat(
    kind: NewChatKind,
    title: &str,
    description: &str,
    user_ids: &[i64],
    self_id: Option<i64>,
    auto_delete: i32,
) -> Result<NewChatRequest, NewChatInvalid> {
    let title = title.trim();
    if title.is_empty() {
        return Err(NewChatInvalid::EmptyTitle);
    }
    if title.chars().count() > TITLE_MAX {
        return Err(NewChatInvalid::TitleTooLong);
    }
    let description = if kind.has_description() {
        description.trim()
    } else {
        ""
    };
    if description.chars().count() > DESCRIPTION_MAX {
        return Err(NewChatInvalid::DescriptionTooLong);
    }
    let mut members = Vec::new();
    if !kind.is_supergroup() {
        for id in user_ids {
            if Some(*id) != self_id && !members.contains(id) {
                members.push(*id);
            }
        }
    }
    Ok(NewChatRequest {
        kind,
        title: title.to_string(),
        description: description.to_string(),
        user_ids: members,
        auto_delete: kind.auto_delete(auto_delete),
    })
}

/// The picker's "3 / 200000" (tdesktop `updateTitle`): everyone already
/// in, plus the picks. A channel shows no count.
pub fn members_counter(
    kind: NewChatKind,
    already_in: i32,
    selected: usize,
    size_max: i32,
) -> Option<String> {
    if kind == NewChatKind::Channel || size_max <= 0 {
        return None;
    }
    let picked = i32::try_from(selected).unwrap_or(i32::MAX);
    let full = already_in.max(1).saturating_add(picked);
    Some(format!(
        "{} / {}",
        group_digits(i64::from(full)),
        group_digits(i64::from(size_max))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAT: ChatId = ChatId(-1_005);

    fn creating(back: NewChatBack) -> NewChatStage {
        NewChatStage::Creating {
            request: RequestId(7),
            back,
        }
    }

    #[test]
    fn a_group_goes_info_members_then_done() {
        let kind = NewChatKind::Group;
        let facts = StageFacts::default();
        assert_eq!(kind.info_confirm(), keys::CREATE_GROUP_NEXT);
        assert_eq!(
            view(kind, &NewChatStage::Info, &facts).step,
            NewChatStep::Info
        );
        let pending = StageFacts {
            create_pending: true,
            ..StageFacts::default()
        };
        let waiting = view(kind, &creating(NewChatBack::Members), &pending);
        assert_eq!(waiting.step, NewChatStep::Members);
        assert!(waiting.busy);
        let made = StageFacts {
            outcome: Some(CreationOutcome::Created(CHAT)),
            ..StageFacts::default()
        };
        assert_eq!(
            view(kind, &creating(NewChatBack::Members), &made).step,
            NewChatStep::Done(CHAT)
        );
    }

    #[test]
    fn a_channel_goes_info_type_subscribers() {
        let kind = NewChatKind::Channel;
        assert_eq!(kind.info_confirm(), keys::CREATE_GROUP_CREATE);
        let made = StageFacts {
            outcome: Some(CreationOutcome::Created(CHAT)),
            ..StageFacts::default()
        };
        assert_eq!(
            view(kind, &creating(NewChatBack::Info), &made).step,
            NewChatStep::Type(CHAT)
        );
        let saving = NewChatStage::Type {
            chat: CHAT,
            saving: Some(vec![RequestId(8)]),
        };
        let out = StageFacts {
            type_pending: true,
            ..StageFacts::default()
        };
        let busy = view(kind, &saving, &out);
        assert_eq!(busy.step, NewChatStep::Type(CHAT));
        assert!(busy.busy);
        let refused = StageFacts {
            type_failed: true,
            ..StageFacts::default()
        };
        assert_eq!(view(kind, &saving, &refused).step, NewChatStep::Type(CHAT));
        assert_eq!(
            view(kind, &saving, &StageFacts::default()).step,
            NewChatStep::Subscribers(CHAT)
        );
    }

    #[test]
    fn a_refusal_returns_to_the_step_with_its_reason() {
        let failed = StageFacts {
            outcome: Some(CreationOutcome::Failed {
                failure: CreateFailure::Restricted,
                reason: "nope".into(),
            }),
            ..StageFacts::default()
        };
        let shown = view(
            NewChatKind::Megagroup,
            &creating(NewChatBack::Info),
            &failed,
        );
        assert_eq!(shown.step, NewChatStep::Info);
        assert_eq!(
            shown.failure,
            Some((CreateFailure::Restricted, "nope".into()))
        );
        assert!(!shown.busy);
    }

    #[test]
    fn server_errors_map_to_tdesktop_messages() {
        let message = |t| CreateFailure::from_server_type(Some(t)).message();
        assert_eq!(message("USERS_TOO_FEW"), Some(keys::CANT_INVITE_PRIVACY));
        assert_eq!(message("PEER_FLOOD"), Some(keys::CANT_INVITE_NOT_CONTACT));
        assert_eq!(message("USER_RESTRICTED"), Some(keys::CANT_DO_THIS));
        assert_eq!(
            CreateFailure::from_server_type(Some("NO_CHAT_TITLE")),
            CreateFailure::NoTitle
        );
        assert_eq!(
            CreateFailure::from_server_type(Some("CHANNELS_TOO_MUCH")),
            CreateFailure::TooManyChannels
        );
        assert_eq!(CreateFailure::from_server_type(None), CreateFailure::Other);
    }

    #[test]
    fn the_request_is_trimmed_and_checked() {
        assert_eq!(
            plan_new_chat(NewChatKind::Group, "  ", "", &[], None, 0),
            Err(NewChatInvalid::EmptyTitle)
        );
        let long = "x".repeat(TITLE_MAX + 1);
        assert_eq!(
            plan_new_chat(NewChatKind::Group, &long, "", &[], None, 0),
            Err(NewChatInvalid::TitleTooLong)
        );
        let about = "y".repeat(DESCRIPTION_MAX + 1);
        assert_eq!(
            plan_new_chat(NewChatKind::Channel, "News", &about, &[], None, 0),
            Err(NewChatInvalid::DescriptionTooLong)
        );
        let group = plan_new_chat(
            NewChatKind::Group,
            " Hikers ",
            "ignored",
            &[5, 1, 5, 6],
            Some(1),
            604_800,
        )
        .unwrap();
        assert_eq!(group.title, "Hikers");
        assert_eq!(group.description, "");
        assert_eq!(group.user_ids, vec![5, 6]);
        assert_eq!(group.auto_delete, 604_800);
        let channel =
            plan_new_chat(NewChatKind::Channel, "News", " Daily ", &[5], None, 604_800).unwrap();
        assert_eq!(channel.description, "Daily");
        assert!(channel.user_ids.is_empty());
        assert_eq!(channel.auto_delete, 0);
        let mega = plan_new_chat(NewChatKind::Megagroup, "Club", "", &[], None, 86_400).unwrap();
        assert_eq!(mega.auto_delete, 86_400);
    }

    #[test]
    fn the_counter_counts_the_viewer() {
        assert_eq!(
            members_counter(NewChatKind::Group, 0, 2, 200_000).as_deref(),
            Some("3 / 200,000")
        );
        assert_eq!(
            members_counter(NewChatKind::Megagroup, 4, 1, 200_000).as_deref(),
            Some("5 / 200,000")
        );
        assert_eq!(members_counter(NewChatKind::Channel, 1, 2, 200_000), None);
        assert_eq!(members_counter(NewChatKind::Group, 0, 2, 0), None);
    }
}

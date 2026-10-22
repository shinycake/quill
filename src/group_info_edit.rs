//! What the group and channel Edit box's Save sends (tdesktop
//! `EditPeerInfoBox` `Controller::save`): the title and the description
//! when they changed, then the chosen photo.
#![deny(clippy::indexing_slicing)]

use crate::profile_photo_sources::ChatPhotoInput;

/// `setChatTitle`: 1-128 characters (schema line 14422, tdesktop
/// `kMaxGroupChannelTitle`).
pub const TITLE_MAX: usize = 128;
/// `setChatDescription`: 0-255 characters (schema line 14530, tdesktop
/// `kMaxChannelDescription`).
pub const DESCRIPTION_MAX: usize = 255;

/// The title and description, trimmed the way tdesktop sends them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupInfoFields {
    pub title: String,
    pub description: String,
}

impl GroupInfoFields {
    pub fn new(title: &str, description: &str) -> Self {
        Self {
            title: title.trim().to_string(),
            description: description.trim().to_string(),
        }
    }
}

/// One request of the save, in the order tdesktop runs its stages.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupInfoEdit {
    Title(String),
    Description(String),
    Photo(ChatPhotoInput),
}

/// Why Save sends nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupInfoInvalid {
    /// tdesktop `validateTitle`: the field shows its error.
    EmptyTitle,
    TitleTooLong,
    DescriptionTooLong,
}

/// The requests that bring `current` (what TDLib last reported) to `now`
/// (the box's fields), then the chosen photo.
pub fn plan_group_info_save(
    current: &GroupInfoFields,
    now: &GroupInfoFields,
    photo: Option<&ChatPhotoInput>,
) -> Result<Vec<GroupInfoEdit>, GroupInfoInvalid> {
    if now.title.is_empty() {
        return Err(GroupInfoInvalid::EmptyTitle);
    }
    if now.title.chars().count() > TITLE_MAX {
        return Err(GroupInfoInvalid::TitleTooLong);
    }
    if now.description.chars().count() > DESCRIPTION_MAX {
        return Err(GroupInfoInvalid::DescriptionTooLong);
    }
    let mut edits = Vec::new();
    if now.title != current.title {
        edits.push(GroupInfoEdit::Title(now.title.clone()));
    }
    if now.description != current.description {
        edits.push(GroupInfoEdit::Description(now.description.clone()));
    }
    if let Some(photo) = photo {
        edits.push(GroupInfoEdit::Photo(photo.clone()));
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::{GroupInfoEdit, GroupInfoFields, GroupInfoInvalid, plan_group_info_save};
    use crate::profile_photo_sources::ChatPhotoInput;

    fn fields(title: &str, description: &str) -> GroupInfoFields {
        GroupInfoFields::new(title, description)
    }

    #[test]
    fn unchanged_fields_send_nothing() {
        let current = fields("Group", "About");
        assert_eq!(
            plan_group_info_save(&current, &fields(" Group ", "About\n"), None),
            Ok(Vec::new())
        );
    }

    #[test]
    fn stages_run_title_description_photo() {
        let photo = ChatPhotoInput::Static {
            path: "/tmp/p.png".into(),
        };
        let plan = plan_group_info_save(
            &fields("Group", "About"),
            &fields("New", ""),
            Some(&photo),
        );
        assert_eq!(
            plan,
            Ok(vec![
                GroupInfoEdit::Title("New".into()),
                GroupInfoEdit::Description(String::new()),
                GroupInfoEdit::Photo(photo),
            ])
        );
    }

    #[test]
    fn limits_are_checked_before_anything_is_sent() {
        let current = fields("Group", "");
        assert_eq!(
            plan_group_info_save(&current, &fields("  ", ""), None),
            Err(GroupInfoInvalid::EmptyTitle)
        );
        assert_eq!(
            plan_group_info_save(&current, &fields(&"t".repeat(129), ""), None),
            Err(GroupInfoInvalid::TitleTooLong)
        );
        assert!(plan_group_info_save(&current, &fields(&"t".repeat(128), ""), None).is_ok());
        assert_eq!(
            plan_group_info_save(&current, &fields("Group", &"d".repeat(256)), None),
            Err(GroupInfoInvalid::DescriptionTooLong)
        );
    }
}

//! The "Your color" editor's model (tdesktop `boxes/peers/edit_peer_color_box.cpp`):
//! the draft of name color, reply icon, profile color and profile icon,
//! which requests applying it needs, and the Premium gate.
//!
//! tdesktop gates the Apply button for the account's own colors: without
//! Premium it opens the Premium preview and sends nothing
//! (`ShowPremiumPreview`). With Premium it sends one request per changed
//! section (`account.updateColor`; TDLib splits that into `setAccentColor`
//! and `setProfileAccentColor`) and closes the box.

use crate::telegram::ProfileAccentColor;
use crate::telegram::name_accent::{BUILT_IN_COUNT, name_color_rgb};

/// The two tabs of the box (`PeerColorTab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeerColorTab {
    #[default]
    Profile,
    Name,
}

/// Name color, reply icon, profile color and profile icon. `-1` is "no
/// profile color", `0` is "no icon".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerColorDraft {
    pub name_color_id: i32,
    pub name_emoji_id: i64,
    pub profile_color_id: i32,
    pub profile_emoji_id: i64,
}

/// What pressing Apply does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// No Premium: open Premium, send nothing.
    NeedsPremium,
    /// Nothing changed: just close.
    Unchanged,
    /// Send `setAccentColor` and/or `setProfileAccentColor`.
    Send { name: bool, profile: bool },
}

impl PeerColorDraft {
    /// The outcome of Apply for a draft against the account's current
    /// colors.
    pub fn apply_outcome(&self, current: &Self, is_premium: bool) -> ApplyOutcome {
        if !is_premium {
            return ApplyOutcome::NeedsPremium;
        }
        let name = self.name_color_id != current.name_color_id
            || self.name_emoji_id != current.name_emoji_id;
        let profile = self.profile_color_id != current.profile_color_id
            || self.profile_emoji_id != current.profile_emoji_id;
        if name || profile {
            ApplyOutcome::Send { name, profile }
        } else {
            ApplyOutcome::Unchanged
        }
    }
}

/// One swatch of the name-color row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameSwatch {
    pub id: i32,
    pub rgb: u32,
}

/// The name-color swatches in server order. Before `updateAccentColors`
/// arrives only the seven built-in colors are known, so those show.
pub fn name_swatches(available_ids: &[i32], dark: bool) -> Vec<NameSwatch> {
    let ids: Vec<i32> = if available_ids.is_empty() {
        (0..BUILT_IN_COUNT).collect()
    } else {
        available_ids.to_vec()
    };
    ids.into_iter()
        .map(|id| NameSwatch {
            id,
            rgb: name_color_rgb(id, dark),
        })
        .collect()
}

/// One swatch of the profile-color row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSwatch {
    pub id: i32,
    pub rgb: u32,
}

/// The profile-color swatches in server order; ids the palette does not
/// describe are skipped (there is no honest color to draw).
pub fn profile_swatches(
    available_ids: &[i32],
    palette: &[ProfileAccentColor],
) -> Vec<ProfileSwatch> {
    available_ids
        .iter()
        .filter_map(|id| {
            palette.iter().find(|c| c.id == *id).map(|c| ProfileSwatch {
                id: *id,
                rgb: c.swatch_rgb(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ApplyOutcome, PeerColorDraft, name_swatches, profile_swatches};
    use crate::telegram::ProfileAccentColor;

    fn draft() -> PeerColorDraft {
        PeerColorDraft {
            name_color_id: 1,
            name_emoji_id: 0,
            profile_color_id: -1,
            profile_emoji_id: 0,
        }
    }

    #[test]
    fn without_premium_apply_only_opens_premium() {
        let mut edited = draft();
        edited.name_color_id = 4;
        assert_eq!(
            edited.apply_outcome(&draft(), false),
            ApplyOutcome::NeedsPremium
        );
        // Even an unchanged draft shows Premium, like tdesktop.
        assert_eq!(
            draft().apply_outcome(&draft(), false),
            ApplyOutcome::NeedsPremium
        );
    }

    #[test]
    fn premium_apply_sends_only_the_changed_sections() {
        let current = draft();
        assert_eq!(
            current.apply_outcome(&current, true),
            ApplyOutcome::Unchanged
        );

        let mut edited = current;
        edited.name_emoji_id = 77;
        assert_eq!(
            edited.apply_outcome(&current, true),
            ApplyOutcome::Send {
                name: true,
                profile: false
            }
        );

        let mut edited = current;
        edited.profile_color_id = 3;
        assert_eq!(
            edited.apply_outcome(&current, true),
            ApplyOutcome::Send {
                name: false,
                profile: true
            }
        );

        let mut edited = current;
        edited.name_color_id = 2;
        edited.profile_emoji_id = 9;
        assert_eq!(
            edited.apply_outcome(&current, true),
            ApplyOutcome::Send {
                name: true,
                profile: true
            }
        );
    }

    #[test]
    fn name_swatches_fall_back_to_the_built_ins() {
        let swatches = name_swatches(&[], false);
        assert_eq!(swatches.len(), 7);
        assert_eq!(swatches[0].id, 0);
        assert_eq!(swatches[0].rgb, 0xcc5049);
        // Server order is kept.
        let ids: Vec<i32> = name_swatches(&[5, 2, 0], true)
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, vec![5, 2, 0]);
    }

    #[test]
    fn profile_swatches_follow_server_order_and_skip_unknown_ids() {
        let palette = vec![
            ProfileAccentColor {
                id: 0,
                light_colors: vec![0xAA0000],
                dark_colors: vec![],
            },
            ProfileAccentColor {
                id: 4,
                light_colors: vec![0x00BB00],
                dark_colors: vec![],
            },
        ];
        let swatches = profile_swatches(&[4, 9, 0], &palette);
        assert_eq!(swatches.len(), 2);
        assert_eq!((swatches[0].id, swatches[0].rgb), (4, 0x00BB00));
        assert_eq!((swatches[1].id, swatches[1].rgb), (0, 0xAA0000));
        assert!(profile_swatches(&[], &palette).is_empty());
    }
}

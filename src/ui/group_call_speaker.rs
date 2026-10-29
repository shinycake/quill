//! A member row's speaking look in the group call window (tdesktop
//! `MembersRow::paintComplexUserpic`): while a participant sounds, two
//! blobs grow around their userpic with the level and the userpic itself
//! shrinks when quiet and swells back as they get louder; the blobs are
//! green while they speak and blue while it is only sound. Levels come
//! from the driver (`connect::group_calls::levels`); between two batches
//! each row eases toward the newest level over tdesktop's
//! `kLevelDuration`, drawn on the frame clock only while some row sounds.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::group_call_panel::{MEMBER_ACTIVE, MEMBER_INACTIVE_STATUS, MEMBER_MUTED_ICON};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::audio_level::{LEVEL_ANIMATION_MS, LevelAnimation};
use quill::calls::speaker_levels::{SpeakerLook, row_blob_radii, row_userpic_scale};
use quill::telegram::envelope::ParsedGroupCallParticipant;
use std::path::Path;
use std::time::Instant;

/// The member list's userpic size.
const USERPIC: f32 = 40.;

/// What one row draws: the driver's look and the eased level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct RowSpeaker {
    pub(super) look: SpeakerLook,
    pub(super) level: f32,
}

/// tdesktop's `applyLastSpoke`: only a participant who may talk sounds.
pub(super) fn can_sound(p: &ParsedGroupCallParticipant) -> bool {
    !p.is_muted_for_all_users || p.can_unmute_self
}

impl QuillApp {
    /// `p`'s speaking look now, with its level eased on the window clock.
    /// Power saving for calls holds the blobs still at rest, as tdesktop
    /// skips row level updates then.
    pub(super) fn group_member_speaker(
        &mut self,
        p: &ParsedGroupCallParticipant,
        now: Instant,
    ) -> RowSpeaker {
        let look = self
            .live
            .as_ref()
            .filter(|_| can_sound(p))
            .map(|live| live.driver.group_call_speaker(p.audio_source_id, now))
            .unwrap_or_default();
        if !look.sounding {
            self.group_call.row_levels.remove(&p.audio_source_id);
            return RowSpeaker { look, level: 0. };
        }
        if quill::power_saving::on(quill::power_saving::Flag::Calls) {
            return RowSpeaker { look, level: 0. };
        }
        let clock = self.group_call_now_ms();
        let animation = self
            .group_call
            .row_levels
            .entry(p.audio_source_id)
            .or_insert_with(|| LevelAnimation::new(0., LEVEL_ANIMATION_MS));
        animation.retarget(look.level, clock);
        RowSpeaker {
            look,
            level: animation.value(clock),
        }
    }

    /// Keep drawing while any row's blobs show (levels move between
    /// batches and the blobs fade in and out); otherwise the window
    /// stays still.
    pub(super) fn tick_group_member_speakers(&self, now: Instant, cx: &mut Context<Self>) {
        let sounding = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.group_call_levels_sounding(now));
        if sounding && !quill::power_saving::on(quill::power_saving::Flag::Calls) {
            self.request_animation_tick(30, cx);
        }
    }
}

/// The blobs' color: green while speaking, blue while it is only sound,
/// red for someone you muted (tdesktop `groupCallMemberActiveStatus`,
/// `groupCallMemberInactiveStatus`, `groupCallMemberMutedIcon`).
fn blob_color(speaking: bool, muted_by_me: bool) -> u32 {
    if muted_by_me {
        MEMBER_MUTED_ICON
    } else if speaking {
        MEMBER_ACTIVE
    } else {
        MEMBER_INACTIVE_STATUS
    }
}

/// The row's userpic with its blobs.
pub(super) fn speaker_userpic(
    name: &str,
    photo: Option<&Path>,
    speaker: RowSpeaker,
    muted_by_me: bool,
) -> AnyElement {
    let enter = speaker.look.enter;
    let scale = if speaker.look.sounding {
        row_userpic_scale(speaker.level, enter)
    } else {
        1.
    };
    let size = USERPIC * scale;
    let color = blob_color(speaker.look.speaking, muted_by_me);
    let blob = |radius: f32, alpha: f32| {
        let center = USERPIC / 2.;
        div()
            .absolute()
            .left(px(center - radius))
            .top(px(center - radius))
            .size(px(radius * 2.))
            .rounded_full()
            .bg(rgb(color))
            .opacity(alpha)
    };
    let (inner, outer) = row_blob_radii(speaker.level, enter);
    div()
        .relative()
        .flex_none()
        .size(px(USERPIC))
        .flex()
        .items_center()
        .justify_center()
        .when(speaker.look.sounding, |this| {
            // `RowBlobs`: the minor blob at half opacity, then the other.
            this.child(blob(inner, 0.5)).child(blob(outer, 0.2))
        })
        .child(chat_avatar(name, photo, size))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{blob_color, can_sound};
    use crate::ui::group_call_panel::{MEMBER_ACTIVE, MEMBER_INACTIVE_STATUS, MEMBER_MUTED_ICON};
    use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

    #[test]
    fn only_those_who_may_talk_sound() {
        let mut p = ParsedGroupCallParticipant {
            participant_id: MessageSender::User { user_id: 9 },
            audio_source_id: 99,
            is_current_user: false,
            is_speaking: false,
            is_hand_raised: false,
            can_be_muted_for_all_users: false,
            can_be_unmuted_for_all_users: false,
            can_be_muted_for_current_user: false,
            can_be_unmuted_for_current_user: false,
            is_muted_for_all_users: true,
            is_muted_for_current_user: false,
            can_unmute_self: false,
            volume_level: 10000,
            order: String::new(),
            video_enabled: false,
            screen_sharing_enabled: false,
            video_info: None,
            screen_sharing_video_info: None,
        };
        assert!(!can_sound(&p), "muted by an admin");
        p.can_unmute_self = true;
        assert!(can_sound(&p), "muted, but may unmute");
        p.is_muted_for_all_users = false;
        assert!(can_sound(&p));
    }

    #[test]
    fn blobs_follow_tdesktops_colors() {
        assert_eq!(blob_color(true, false), MEMBER_ACTIVE);
        assert_eq!(blob_color(false, false), MEMBER_INACTIVE_STATUS);
        assert_eq!(blob_color(true, true), MEMBER_MUTED_ICON);
    }
}

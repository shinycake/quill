//! macOS: `MPRemoteCommandCenter` for the media keys and
//! `MPNowPlayingInfoCenter` for the Now Playing widget, as Telegram
//! Desktop's `platform/mac` media controls do.
//!
//! Called from the main thread (the UI tick). The command handlers run on
//! a MediaPlayer thread and only push into the shared queue.

use super::{Command, NowPlaying, push_command};
use block2::RcBlock;
use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemPropertyArtist,
    MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate,
    MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter, MPRemoteCommandEvent,
    MPRemoteCommandHandlerStatus,
};
use std::ptr::NonNull;
use std::sync::Once;

static REGISTER: Once = Once::new();

fn handler(
    command: Command,
) -> RcBlock<dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus> {
    RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        push_command(command);
        MPRemoteCommandHandlerStatus::Success
    })
}

fn hook(remote: &MPRemoteCommand, command: Command) {
    // SAFETY: plain Objective-C calls on live objects; the block is copied
    // by the command and the returned target is kept alive by it.
    unsafe {
        remote.setEnabled(true);
        let target = remote.addTargetWithHandler(&handler(command));
        // The registration lives as long as the process.
        std::mem::forget(target);
    }
}

fn register() {
    REGISTER.call_once(|| {
        // SAFETY: shared singletons; see `hook`.
        unsafe {
            let center = MPRemoteCommandCenter::sharedCommandCenter();
            hook(&center.playCommand(), Command::Play);
            hook(&center.pauseCommand(), Command::Pause);
            hook(&center.togglePlayPauseCommand(), Command::Toggle);
            hook(&center.stopCommand(), Command::Stop);
            hook(&center.nextTrackCommand(), Command::Next);
            hook(&center.previousTrackCommand(), Command::Previous);
            let seek = center.changePlaybackPositionCommand();
            seek.setEnabled(true);
            let block = RcBlock::new(|event: NonNull<MPRemoteCommandEvent>| {
                // The command only delivers this event type.
                let event = event.cast::<MPChangePlaybackPositionCommandEvent>();
                push_command(Command::SeekTo(event.as_ref().positionTime()));
                MPRemoteCommandHandlerStatus::Success
            });
            std::mem::forget(seek.addTargetWithHandler(&block));
        }
    });
}

fn any<T: Message>(object: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an `AnyObject`.
    unsafe { Retained::cast_unchecked(object) }
}

fn number(value: f64) -> Retained<AnyObject> {
    any(NSNumber::new_f64(value))
}

pub(super) fn set(info: Option<&NowPlaying>) {
    register();
    // SAFETY: framework singletons and constants; every value put in the
    // dictionary is an NSString or NSNumber.
    unsafe {
        let center = MPNowPlayingInfoCenter::defaultCenter();
        let Some(info) = info else {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            return;
        };
        let commands = MPRemoteCommandCenter::sharedCommandCenter();
        commands.nextTrackCommand().setEnabled(info.can_next);
        commands
            .previousTrackCommand()
            .setEnabled(info.can_previous);
        let title = any(NSString::from_str(&info.title));
        let artist = any(NSString::from_str(&info.artist));
        let rate = if info.playing { info.rate } else { 0.0 };
        let keys = [
            MPMediaItemPropertyTitle,
            MPMediaItemPropertyArtist,
            MPMediaItemPropertyPlaybackDuration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            MPNowPlayingInfoPropertyPlaybackRate,
        ];
        let values = [
            title,
            artist,
            number(info.duration_secs),
            number(info.position_secs),
            number(rate),
        ];
        let dictionary: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_retained_objects(&keys, &values);
        center.setNowPlayingInfo(Some(&dictionary));
        center.setPlaybackState(if info.playing {
            MPNowPlayingPlaybackState::Playing
        } else {
            MPNowPlayingPlaybackState::Paused
        });
    }
}

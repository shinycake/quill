//! Windows: `SystemMediaTransportControls` (the media overlay, lock screen
//! and keyboard media keys), as Telegram Desktop's
//! `platform/win/specific_win.cpp` media controls do.
//!
//! The controls come from a `MediaPlayer` whose own command manager is
//! switched off, which is the way to get an SMTC without a window handle.
//! Everything runs on one worker thread (MTA); [`set`] only sends the new
//! state over a channel so the UI never waits on WinRT. When any of it
//! fails (older Windows, no media foundation) the worker drains quietly and
//! the media keys are simply not wired.

use super::{Command, NowPlaying, push_command};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::HSTRING;

static WORKER: OnceLock<Sender<Option<NowPlaying>>> = OnceLock::new();

pub(super) fn set(info: Option<&NowPlaying>) {
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<Option<NowPlaying>>();
        let _ = std::thread::Builder::new()
            .name("smtc".into())
            .spawn(move || run(rx));
        tx
    });
    let _ = sender.send(info.cloned());
}

fn run(rx: Receiver<Option<NowPlaying>>) {
    // SAFETY: first COM call on this thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let Ok((_player, controls)) = create() else {
        while rx.recv().is_ok() {}
        return;
    };
    while let Ok(info) = rx.recv() {
        let info = rx.try_iter().last().unwrap_or(info);
        let _ = apply(&controls, info.as_ref());
    }
}

fn create() -> windows::core::Result<(MediaPlayer, SystemMediaTransportControls)> {
    let player = MediaPlayer::new()?;
    player.CommandManager()?.SetIsEnabled(false)?;
    let controls = player.SystemMediaTransportControls()?;
    controls.SetIsEnabled(true)?;
    controls.ButtonPressed(&TypedEventHandler::new(
        |_, args: windows::core::Ref<'_, SystemMediaTransportControlsButtonPressedEventArgs>| {
            if let Some(args) = args.as_ref() {
                let command = match args.Button()? {
                    SystemMediaTransportControlsButton::Play => Some(Command::Play),
                    SystemMediaTransportControlsButton::Pause => Some(Command::Pause),
                    SystemMediaTransportControlsButton::Stop => Some(Command::Stop),
                    SystemMediaTransportControlsButton::Next => Some(Command::Next),
                    SystemMediaTransportControlsButton::Previous => Some(Command::Previous),
                    _ => None,
                };
                if let Some(command) = command {
                    push_command(command);
                }
            }
            Ok(())
        },
    ))?;
    controls.PlaybackPositionChangeRequested(&TypedEventHandler::new(
        |_, args: windows::core::Ref<'_, PlaybackPositionChangeRequestedEventArgs>| {
            if let Some(args) = args.as_ref() {
                let ticks = args.RequestedPlaybackPosition()?.Duration;
                push_command(Command::SeekTo(ticks as f64 / 1e7));
            }
            Ok(())
        },
    ))?;
    Ok((player, controls))
}

fn span(secs: f64) -> TimeSpan {
    TimeSpan {
        Duration: (secs.max(0.0) * 1e7) as i64,
    }
}

fn apply(
    controls: &SystemMediaTransportControls,
    info: Option<&NowPlaying>,
) -> windows::core::Result<()> {
    let updater = controls.DisplayUpdater()?;
    let Some(info) = info else {
        controls.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
        updater.ClearAll()?;
        updater.Update()?;
        return Ok(());
    };
    controls.SetIsPlayEnabled(true)?;
    controls.SetIsPauseEnabled(true)?;
    controls.SetIsStopEnabled(true)?;
    controls.SetIsNextEnabled(info.can_next)?;
    controls.SetIsPreviousEnabled(info.can_previous)?;
    updater.SetType(MediaPlaybackType::Music)?;
    let music = updater.MusicProperties()?;
    music.SetTitle(&HSTRING::from(info.title.as_str()))?;
    music.SetArtist(&HSTRING::from(info.artist.as_str()))?;
    updater.Update()?;
    controls.SetPlaybackStatus(if info.playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    })?;
    let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
    timeline.SetStartTime(span(0.0))?;
    timeline.SetMinSeekTime(span(0.0))?;
    timeline.SetEndTime(span(info.duration_secs))?;
    timeline.SetMaxSeekTime(span(info.duration_secs))?;
    timeline.SetPosition(span(info.position_secs))?;
    controls.UpdateTimelineProperties(&timeline)?;
    Ok(())
}

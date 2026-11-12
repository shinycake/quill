#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The volume is one value (sweep 12-A021): mute keeps the level it left,
//! a slider dragged to zero is mute with that restore level, and unmute
//! never lands on silence.

use crate::ui::players_state::Volume;

#[test]
fn the_mute_click_restores_the_level_it_left() {
    let muted = Volume::Level(0.6).toggled();
    assert_eq!(muted.level(), 0.0);
    assert!(muted.is_muted());
    assert_eq!(muted.toggled(), Volume::Level(0.6));
}

#[test]
fn a_slider_dragged_to_zero_unmutes_to_the_level_before() {
    let dragged = Volume::Level(0.4).set(0.0);
    assert_eq!(dragged, Volume::Muted { restore: 0.4 });
    assert_eq!(dragged.toggled(), Volume::Level(0.4));
    // Dragging further down while muted keeps the restore level.
    assert_eq!(dragged.set(0.0), Volume::Muted { restore: 0.4 });
}

#[test]
fn unmute_never_lands_on_silence() {
    let stale = Volume::Muted { restore: 0.0 };
    assert_eq!(stale.toggled(), Volume::Level(1.0));
    assert_eq!(Volume::Level(0.5).set(f32::NAN), Volume::Level(0.5));
    assert_eq!(Volume::Muted { restore: 0.3 }.set(0.8), Volume::Level(0.8));
}

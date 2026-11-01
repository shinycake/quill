//! The first-run onboarding's state: the intro clip, then the notice that
//! Quill is experimental, then the highlights, then sign-in. The screen is
//! drawn from this value alone (`ui/first_run_ui.rs`).
#![deny(clippy::indexing_slicing)]

use super::motion::{elapsed_since, progress};
use super::video_player::{PlayState, Transport, VideoPlayer};
use std::time::{Duration, Instant};

/// How long the clip takes to rise out of black.
pub(super) const FADE_IN: Duration = Duration::from_millis(900);
/// The way out: the clip sinks to black over the first half, then the
/// black lifts off the notice over the second. It starts in the clip's
/// last moments, or when it is skipped, and then runs on the wall clock.
pub(super) const OUTRO: Duration = Duration::from_millis(1800);
/// A clip that has not shown a picture by then gives way to the notice.
const OPEN_TIMEOUT: Duration = Duration::from_secs(4);
/// How long a card takes to rise into place.
pub(super) const CARD_IN: Duration = Duration::from_millis(320);

#[derive(Default)]
pub(super) enum FirstRun {
    /// The intro clip over the whole window, then the notice. `intro` is
    /// `None` while the clip is `writing` out for the player (or was
    /// skipped before it was), and when it could not open (or animations
    /// are off): the notice then shows at once.
    Welcome {
        intro: Option<VideoPlayer>,
        writing: bool,
        opened_at: Instant,
        outro: Option<Outro>,
    },
    Highlights {
        since: Instant,
    },
    /// Finished, or never needed: the sign-in screen shows.
    #[default]
    Done,
}

/// The way out of the clip, once it started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Outro {
    pub(super) from: Instant,
    /// How dark the clip still was when it started (a skip while it rose
    /// out of black keeps that darkness).
    pub(super) dark: f32,
}

/// How the intro looks at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct IntroLook {
    /// Escape or a click still skips the clip.
    pub(super) skippable: bool,
    /// Black over the clip: 1 hides it.
    pub(super) veil: f32,
    /// The black cover over the whole window: 0 is gone.
    pub(super) curtain: f32,
    /// The notice under the cover.
    pub(super) card: f32,
    pub(super) volume: f32,
}

impl IntroLook {
    const NOTICE: Self = Self {
        skippable: false,
        veil: 1.,
        curtain: 0.,
        card: 1.,
        volume: 0.,
    };
}

impl FirstRun {
    pub(super) fn welcome(intro: Option<VideoPlayer>, now: Instant) -> Self {
        Self::Welcome {
            intro,
            writing: false,
            opened_at: now,
            outro: None,
        }
    }

    /// The intro on black while its clip is written out off the UI thread;
    /// [`Self::intro_written`] hands it the player.
    pub(super) fn writing_intro(now: Instant) -> Self {
        Self::Welcome {
            intro: None,
            writing: true,
            opened_at: now,
            outro: None,
        }
    }

    /// The clip is written and opened (`None`: it could not be). A way out
    /// that already started (a skip, the open timeout) runs to its end on
    /// black without it.
    pub(super) fn intro_written(&mut self, player: Option<VideoPlayer>) {
        if let Self::Welcome {
            intro,
            writing: writing @ true,
            outro: None,
            ..
        } = self
        {
            *writing = false;
            *intro = player;
        }
    }

    /// The intro is on black, waiting for its clip to be written.
    pub(super) fn awaits_intro(&self) -> bool {
        matches!(
            self,
            Self::Welcome {
                writing: true,
                outro: None,
                ..
            }
        )
    }

    /// Where the clip is: a clip still being written is still opening.
    fn intro_transport(&self) -> Option<Transport> {
        match self {
            Self::Welcome {
                intro: Some(player),
                ..
            } => Some(player.transport()),
            Self::Welcome { writing: true, .. } => Some(Transport {
                state: PlayState::Opening,
                position: 0.,
                duration: None,
                buffered: Vec::new(),
                rate: 1.,
                volume: 1.,
                has_sound: false,
                live: false,
            }),
            _ => None,
        }
    }

    pub(super) fn intro_skippable(&self, now: Instant) -> bool {
        self.intro_look(now).skippable
    }

    /// Skip the clip: the way out starts now.
    pub(super) fn skip_intro(&mut self, now: Instant) {
        let Some(transport) = self.intro_transport() else {
            return;
        };
        if let Self::Welcome {
            outro: outro @ None,
            ..
        } = self
        {
            *outro = Some(Outro {
                from: now,
                dark: 1. - rising(&transport),
            });
        }
    }

    /// Start the way out when the clip reaches its last moments, ends,
    /// fails or never opens; then the clock runs it, whatever the player
    /// does. Once the clip is black it stops playing.
    pub(super) fn settle(&mut self, now: Instant) {
        let Some(transport) = self.intro_transport() else {
            return;
        };
        let Self::Welcome {
            intro,
            opened_at,
            outro,
            ..
        } = self
        else {
            return;
        };
        if outro.is_none() {
            *outro = outro_start(&transport, *opened_at, now);
        }
        if let Some(player) = intro
            && outro.is_some_and(|o| sunk(o, now) >= 1.)
            && player.wants_play()
        {
            player.pause();
        }
    }

    /// The notice was acknowledged: the clip's player goes away with it.
    pub(super) fn acknowledge(&mut self, now: Instant) {
        if matches!(self, Self::Welcome { .. }) {
            *self = Self::Highlights { since: now };
        }
    }

    pub(super) fn finish(&mut self) {
        *self = Self::Done;
    }

    pub(super) fn intro_player(&self) -> Option<&VideoPlayer> {
        match self {
            Self::Welcome {
                intro: Some(player),
                ..
            } => Some(player),
            _ => None,
        }
    }

    pub(super) fn intro_look(&self, now: Instant) -> IntroLook {
        match (self, self.intro_transport()) {
            (Self::Welcome { outro, .. }, Some(transport)) => intro_look(&transport, *outro, now),
            _ => IntroLook::NOTICE,
        }
    }

    /// A transition is running that no video frame drives, or the clip
    /// has not shown a picture yet (the open timeout needs a clock).
    pub(super) fn animating(&self, now: Instant) -> bool {
        match (self, self.intro_transport()) {
            (
                Self::Welcome {
                    outro: Some(outro), ..
                },
                Some(_),
            ) => elapsed_since(outro.from, now) < OUTRO,
            (Self::Welcome { outro: None, .. }, Some(transport)) => {
                matches!(transport.state, PlayState::Opening | PlayState::Ready)
            }
            (Self::Highlights { since }, _) => elapsed_since(*since, now) < CARD_IN,
            _ => false,
        }
    }
}

/// The intro at one moment.
pub(super) fn intro_look(transport: &Transport, outro: Option<Outro>, now: Instant) -> IntroLook {
    let Some(outro) = outro else {
        return IntroLook {
            skippable: true,
            veil: 1. - rising(transport),
            curtain: 1.,
            card: 0.,
            volume: 1.,
        };
    };
    let out = progress(elapsed_since(outro.from, now), OUTRO);
    let sunk = sunk(outro, now);
    let lifted = (out * 2. - 1.).clamp(0., 1.);
    IntroLook {
        skippable: false,
        veil: sunk.max(outro.dark),
        curtain: 1. - lifted,
        card: lifted,
        volume: 1. - sunk,
    }
}

fn sunk(outro: Outro, now: Instant) -> f32 {
    (progress(elapsed_since(outro.from, now), OUTRO) * 2.).clamp(0., 1.)
}

/// How far the clip has risen out of black.
fn rising(transport: &Transport) -> f32 {
    match transport.state {
        PlayState::Opening | PlayState::Ready => 0.,
        _ => (transport.position / FADE_IN.as_secs_f64()).clamp(0., 1.) as f32,
    }
}

/// When the way out starts on its own, if it does now: in the clip's last
/// [`OUTRO`] (backdated to match its position), when it ends, or at once
/// to the notice when it fails or never opens.
pub(super) fn outro_start(
    transport: &Transport,
    opened_at: Instant,
    now: Instant,
) -> Option<Outro> {
    let already = |part: f64| {
        let back = OUTRO.mul_f64(part.clamp(0., 1.));
        Some(Outro {
            from: now.checked_sub(back).unwrap_or(now),
            dark: 1. - rising(transport),
        })
    };
    match transport.state {
        PlayState::Failed => already(1.),
        PlayState::Opening | PlayState::Ready => {
            (elapsed_since(opened_at, now) >= OPEN_TIMEOUT).then_some(())?;
            already(1.)
        }
        PlayState::Ended => already(0.),
        PlayState::Playing | PlayState::Buffering | PlayState::Paused => {
            let total = transport.duration.filter(|d| *d > 0.)?;
            let span = OUTRO.as_secs_f64().min(total);
            let part = (transport.position - (total - span)) / span;
            (part > 0.).then_some(())?;
            already(part)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::video_player::{PlayState, Transport};
    use super::{FirstRun, OUTRO, Outro, intro_look, outro_start};
    use std::time::{Duration, Instant};

    fn transport(state: PlayState, position: f64) -> Transport {
        Transport {
            state,
            position,
            duration: Some(6.),
            buffered: Vec::new(),
            rate: 1.,
            volume: 1.,
            has_sound: true,
            live: false,
        }
    }

    #[test]
    fn the_clip_rises_from_black_while_it_plays() {
        let now = Instant::now();
        let look = |state, position| intro_look(&transport(state, position), None, now);
        let opening = look(PlayState::Opening, 0.);
        assert!(opening.skippable && opening.veil == 1. && opening.curtain == 1.);
        let rising = look(PlayState::Playing, 0.45);
        assert!(rising.veil > 0.4 && rising.veil < 0.6, "{rising:?}");
        let playing = look(PlayState::Playing, 3.);
        assert_eq!((playing.veil, playing.card, playing.volume), (0., 0., 1.));
    }

    #[test]
    fn the_way_out_starts_near_the_end_and_then_runs_on_the_clock() {
        let opened = Instant::now();
        let now = opened + Duration::from_secs(6);
        assert_eq!(
            outro_start(&transport(PlayState::Playing, 3.), opened, now),
            None
        );
        let outro = outro_start(&transport(PlayState::Playing, 4.65), opened, now)
            .expect("in the last moments");
        let sinking = intro_look(&transport(PlayState::Playing, 4.65), Some(outro), now);
        assert!(
            !sinking.skippable && sinking.veil > 0.4 && sinking.card == 0.,
            "{sinking:?}"
        );
        // The player stalls in its last moments: the clock still lifts the
        // black off the notice.
        let stalled = transport(PlayState::Buffering, 4.7);
        let lifting = intro_look(&stalled, Some(outro), now + OUTRO / 2);
        assert!(lifting.curtain > 0. && lifting.curtain < 1., "{lifting:?}");
        let done = intro_look(&stalled, Some(outro), now + OUTRO);
        assert_eq!((done.curtain, done.card, done.volume), (0., 1., 0.));
    }

    #[test]
    fn a_clip_that_fails_or_never_opens_gives_way_to_the_notice() {
        let opened = Instant::now();
        let failed = outro_start(&transport(PlayState::Failed, 0.), opened, opened);
        let look = intro_look(&transport(PlayState::Failed, 0.), failed, opened);
        assert_eq!((look.curtain, look.card), (0., 1.));
        let opening = transport(PlayState::Opening, 0.);
        assert_eq!(
            outro_start(&opening, opened, opened + Duration::from_secs(1)),
            None
        );
        let late = opened + Duration::from_secs(5);
        let gave_up = intro_look(&opening, outro_start(&opening, opened, late), late);
        assert_eq!(gave_up.card, 1.);
    }

    #[test]
    fn a_skip_while_rising_keeps_the_clip_dark() {
        let at = Instant::now();
        let outro = Some(Outro {
            from: at,
            dark: 0.8,
        });
        let skipped = intro_look(&transport(PlayState::Playing, 0.2), outro, at);
        assert_eq!(skipped.veil, 0.8);
        let half = intro_look(&transport(PlayState::Playing, 2.), outro, at + OUTRO / 4);
        assert!(half.veil >= 0.8 && half.volume < 0.6, "{half:?}");
    }

    #[test]
    fn without_a_clip_the_notice_shows_and_steps_follow_in_order() {
        let now = Instant::now();
        let mut first_run = FirstRun::welcome(None, now);
        assert_eq!(first_run.intro_look(now).card, 1.);
        assert!(!first_run.intro_skippable(now));
        first_run.acknowledge(now);
        assert!(matches!(first_run, FirstRun::Highlights { .. }));
        first_run.acknowledge(now);
        assert!(
            matches!(first_run, FirstRun::Highlights { .. }),
            "only the notice acknowledges"
        );
        first_run.finish();
        assert!(matches!(first_run, FirstRun::Done));
    }

    /// The clip is written out off the UI thread: meanwhile the intro is
    /// black and skippable, a clip that cannot be written gives way to the
    /// notice, and a skip runs its way out on black.
    #[test]
    fn the_intro_waits_on_black_while_its_clip_is_written() {
        let now = Instant::now();
        let mut writing = FirstRun::writing_intro(now);
        assert!(writing.awaits_intro());
        let look = writing.intro_look(now);
        assert!(look.skippable && look.curtain == 1. && look.veil == 1.);
        assert!(writing.animating(now), "the open timeout needs a clock");
        writing.intro_written(None);
        assert!(!writing.awaits_intro());
        assert_eq!(writing.intro_look(now).card, 1., "the notice");

        let mut skipped = FirstRun::writing_intro(now);
        skipped.skip_intro(now);
        assert!(!skipped.awaits_intro(), "no player is wanted any more");
        skipped.intro_written(None);
        let lifting = skipped.intro_look(now + OUTRO * 3 / 4);
        assert!(lifting.curtain > 0. && lifting.curtain < 1., "{lifting:?}");
        assert_eq!(skipped.intro_look(now + OUTRO).card, 1.);

        let mut slow = FirstRun::writing_intro(now);
        let late = now + Duration::from_secs(5);
        slow.settle(late);
        assert!(!slow.awaits_intro(), "the open timeout covers the write");
    }
}

//! Making a group call's instance and its join payload off the UI thread.
//! tdesktop takes the payload through `emitJoinPayload`'s callback; the
//! library's `qc_group_join_payload` waits for it, up to 2 s, so here the wait
//! gets a thread. The answer waits in a channel for the poll loop.
//!
//! The receiver is the offer's generation. Whoever stops wanting an offer
//! drops it, and whatever the thread made then has nobody to reach, so it
//! retires itself through [`Unclaimed`]. That holds however late the thread
//! answers, and with the engine itself gone.
use super::*;
use crate::thread_guard::spawn_guarded;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

/// A made instance, and how to end it if nobody takes it.
pub(super) struct Unclaimed<T> {
    value: Option<T>,
    discard: Option<Box<dyn FnOnce(T) + Send>>,
}

impl<T> Unclaimed<T> {
    fn new(value: T, discard: impl FnOnce(T) + Send + 'static) -> Self {
        Self {
            value: Some(value),
            discard: Some(Box::new(discard)),
        }
    }

    fn claim(mut self) -> Option<T> {
        self.value.take()
    }
}

impl<T> Drop for Unclaimed<T> {
    fn drop(&mut self) {
        if let (Some(value), Some(discard)) = (self.value.take(), self.discard.take()) {
            discard(value);
        }
    }
}

/// One offer being made. Dropping it abandons the offer.
pub(super) struct OfferRun<T> {
    answer: Receiver<T>,
}

impl<T: Send + 'static> OfferRun<T> {
    fn start(
        reaper: &Reaper,
        make: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, EngineError> {
        let (answers, answer) = channel();
        let hold = reaper.hold();
        spawn_guarded("quill-call-offer", move || {
            let _hold = hold;
            let answering = Answering(Some(answers));
            answering.send(make());
        })
        .map_err(|_| offer_failed())?;
        Ok(Self { answer })
    }

    /// The answer, once; `Some(None)` when the thread died without one.
    fn answer(&self) -> Option<Option<T>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(Some(answer)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }

    #[cfg(test)]
    fn wait(self) -> Option<T> {
        self.answer.recv().ok()
    }
}

/// Whether it answers or panics, the poll loop is poked after the answer or
/// the hang-up is there to read.
struct Answering<T>(Option<Sender<T>>);

impl<T> Answering<T> {
    fn send(mut self, answer: T) {
        if let Some(to) = self.0.take() {
            let _ = to.send(answer);
        }
    }
}

impl<T> Drop for Answering<T> {
    fn drop(&mut self) {
        drop(self.0.take());
        crate::wake::UPDATES.poke();
    }
}

type Made<T> = Option<(Unclaimed<T>, String)>;
type MadeCall = (Arc<GroupContext>, Native<qc_group>);

fn offer_failed() -> EngineError {
    EngineError::Engine {
        op: "qc_group_create",
        code: -1,
    }
}

/// What the [`GroupCall`] will hold besides the instance.
pub(super) struct CallLanding {
    pub(super) chat_id: i64,
    pub(super) role: GroupCallRole,
    pub(super) crypto: Option<Arc<dyn FrameCrypto>>,
}

pub(super) struct CallOffer {
    pub(super) run: OfferRun<Made<MadeCall>>,
    pub(super) landing: CallLanding,
}

#[derive(Default)]
pub(in crate::calls::engine::tgcalls) struct GroupOffers {
    pub(super) calls: HashMap<i32, CallOffer>,
    pub(super) screens: HashMap<i32, OfferRun<Made<Presentation>>>,
}

impl GroupOffers {
    pub(super) fn forget(&mut self, group_call_id: i32) {
        self.calls.remove(&group_call_id);
        self.screens.remove(&group_call_id);
    }
}

pub(super) fn make_call(
    api: &'static Loader,
    reaper: &Arc<Reaper>,
    context: Arc<GroupContext>,
) -> Result<OfferRun<Made<MadeCall>>, EngineError> {
    let keeper = Arc::clone(reaper);
    OfferRun::start(reaper, move || {
        let (handle, payload) = create_instance(api, &keeper, &context, std::ptr::null_mut())?;
        let made = Unclaimed::new((context, handle), move |(context, handle)| {
            context.retire();
            close_conference(&context);
            keeper.hand_off(move || {
                // SAFETY: the instance made here, destroyed once and never
                // from its own callback; the context outlives it.
                unsafe { (api.qc_group_destroy)(handle.as_ptr()) };
                drop(context);
            });
        });
        Some((made, payload))
    })
}

pub(super) fn make_presentation(
    api: &'static Loader,
    reaper: &Arc<Reaper>,
    context: Arc<GroupContext>,
) -> Result<OfferRun<Made<Presentation>>, EngineError> {
    let keeper = Arc::clone(reaper);
    OfferRun::start(reaper, move || {
        // SAFETY: returns a new external screencast source owned here.
        let screen = unsafe { (api.qc_video_capture_external)(1) };
        let screen = Native::new(screen)?;
        let Some((handle, payload)) = create_instance(api, &keeper, &context, screen.as_ptr())
        else {
            // SAFETY: Quill's only reference to the source made above.
            unsafe { (api.qc_video_capture_free)(screen.as_ptr()) };
            return None;
        };
        let presentation = Presentation {
            context,
            handle,
            screen,
            connected: false,
        };
        let made = Unclaimed::new(presentation, move |presentation| {
            presentation.retire();
            keeper.hand_off(move || presentation.destroy(api));
        });
        Some((made, payload))
    })
}

impl TgcallsEngine {
    /// Takes in the instances that were made, and reports each offer.
    pub(in crate::calls::engine::tgcalls) fn land_group_offers(&mut self) -> Vec<GroupOffer> {
        let mut landed = Vec::new();
        let calls: Vec<_> = self
            .offers
            .calls
            .iter()
            .filter_map(|(id, offer)| Some((*id, offer.run.answer()?)))
            .collect();
        for (id, answer) in calls {
            if let Some(offer) = self.offers.calls.remove(&id) {
                landed.push(GroupOffer {
                    key: GroupOfferKey::Call(id),
                    payload: self.land_call(id, offer.landing, answer.flatten()),
                });
            }
        }
        let screens: Vec<_> = self
            .offers
            .screens
            .iter()
            .filter_map(|(id, run)| Some((*id, run.answer()?)))
            .collect();
        for (id, answer) in screens {
            self.offers.screens.remove(&id);
            landed.push(GroupOffer {
                key: GroupOfferKey::Screen(id),
                payload: self.land_screen(id, answer.flatten()),
            });
        }
        landed
    }

    fn land_call(
        &mut self,
        id: i32,
        landing: CallLanding,
        made: Made<MadeCall>,
    ) -> Result<String, EngineError> {
        let (made, payload) = made.ok_or_else(offer_failed)?;
        let (context, handle) = made.claim().ok_or_else(offer_failed)?;
        self.land_group_call(id, landing, context, handle);
        Ok(payload)
    }

    fn land_screen(&mut self, id: i32, made: Made<Presentation>) -> Result<String, EngineError> {
        let (made, payload) = made.ok_or_else(offer_failed)?;
        let api = self.api;
        let reaper = Arc::clone(&self.reaper);
        let call = self.group_mut(id)?;
        let presentation = made.claim().ok_or_else(offer_failed)?;
        stop_presentation(api, &reaper, call);
        call.presentation = Some(presentation);
        Ok(payload)
    }

    /// Blocks until `id`'s instance is made, and lands it.
    #[cfg(test)]
    pub(in crate::calls::engine::tgcalls) fn wait_group_offer(
        &mut self,
        id: i32,
    ) -> Result<String, EngineError> {
        let offer = self
            .offers
            .calls
            .remove(&id)
            .ok_or(EngineError::NoSuchCall(id))?;
        self.land_call(id, offer.landing, offer.run.wait().flatten())
    }

    /// Blocks until `id`'s presentation is made, and lands it.
    #[cfg(test)]
    pub(in crate::calls::engine::tgcalls) fn wait_screen_offer(
        &mut self,
        id: i32,
    ) -> Result<String, EngineError> {
        let run = self
            .offers
            .screens
            .remove(&id)
            .ok_or(EngineError::NoSuchCall(id))?;
        self.land_screen(id, run.wait().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender, channel};

    fn offer_held_by(
        reaper: &Reaper,
        release: Receiver<()>,
        discarded: Sender<&'static str>,
    ) -> OfferRun<Option<Unclaimed<&'static str>>> {
        OfferRun::start(reaper, move || {
            release.recv().ok()?;
            Some(Unclaimed::new("instance", move |value| {
                let _ = discarded.send(value);
            }))
        })
        .expect("the thread starts")
    }

    #[test]
    fn a_slow_offer_holds_nobody_and_lands_when_it_is_made() {
        let reaper = Reaper::start();
        let (release, released) = channel();
        let (discarded, discards) = channel();
        let run = offer_held_by(&reaper, released, discarded);
        assert!(run.answer().is_none(), "the caller is back before tgcalls");
        let _ = release.send(());
        let made = run.wait().flatten().expect("the offer lands");
        assert_eq!(made.claim(), Some("instance"));
        assert!(discards.try_recv().is_err(), "a claimed instance lives on");
    }

    #[test]
    fn an_abandoned_offer_ends_its_instance_however_late_it_is_made() {
        let reaper = Reaper::start();
        let (release, released) = channel();
        let (discarded, discards) = channel();
        let run = offer_held_by(&reaper, released, discarded);
        drop(run);
        let _ = release.send(());
        assert_eq!(discards.recv(), Ok("instance"));
    }

    #[test]
    fn an_offer_made_and_then_abandoned_ends_its_instance() {
        let reaper = Reaper::start();
        let (release, released) = channel();
        let (discarded, discards) = channel();
        let run = offer_held_by(&reaper, released, discarded);
        let _ = release.send(());
        reaper.wait_drained();
        drop(run);
        assert_eq!(discards.recv(), Ok("instance"));
    }

    #[test]
    fn quit_waits_for_an_offer_being_made() {
        let reaper = Reaper::start();
        let (release, released) = channel();
        let (discarded, _discards) = channel();
        let _run = offer_held_by(&reaper, released, discarded);
        assert!(!reaper.is_drained(), "the offer is still running");
        let _ = release.send(());
        reaper.wait_drained();
    }
}

//! What the chat has told TDLib it viewed (`viewMessages`): one stage per
//! message instead of a viewed set, an in-flight map and a due set, and one
//! phase for whether rows were reported, which owns the rows on screen.
use crate::ids::RequestId;
use std::collections::{BTreeMap, BTreeSet};

/// Where one message stands with `viewMessages`. A message nobody reported
/// has no stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewStage {
    /// Reported as shown and not sent yet.
    Due,
    /// Sent as this `viewMessages`; its answer settles it.
    InFlight(RequestId),
    /// TDLib accepted it.
    Done,
}

/// The view bookkeeping of one history, in one of two phases: before the
/// UI has reported shown rows only the newest row is viewed; from the first
/// report on only reported rows are, and the report's rows are kept.
#[derive(Debug)]
pub struct ViewBook {
    phase: Phase,
    /// How many times the history was opened afresh (`restart`): a report
    /// the UI de-duplicates by its rows is only a repeat within one open.
    epoch: u64,
}

#[derive(Debug)]
enum Phase {
    /// No report yet this open generation.
    Newest(Stages),
    /// From now on only reported rows are viewed.
    Reported {
        stages: Stages,
        /// The rows of the UI's latest report: what is on screen now. A row
        /// here that gets a new unread reaction is viewed again, which
        /// reads the reaction (tdesktop `HistoryInner::paintEvent` →
        /// `markContentsRead` for a shown `hasUnreadReaction` item).
        on_screen: BTreeSet<i64>,
    },
}

type Stages = BTreeMap<i64, ViewStage>;

impl Default for ViewBook {
    fn default() -> Self {
        Self {
            phase: Phase::Newest(Stages::new()),
            epoch: 0,
        }
    }
}

impl ViewBook {
    fn stages(&self) -> &Stages {
        match &self.phase {
            Phase::Newest(stages) | Phase::Reported { stages, .. } => stages,
        }
    }

    fn stages_mut(&mut self) -> &mut Stages {
        match &mut self.phase {
            Phase::Newest(stages) | Phase::Reported { stages, .. } => stages,
        }
    }

    /// A fresh open generation: nothing viewed or reported.
    pub fn restart(&mut self) {
        *self = Self {
            epoch: self.epoch.saturating_add(1),
            ..Self::default()
        };
    }

    /// Which open of the history this is (changes on every `restart`).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// A thread's rows are viewed as reported from the start: no stage is
    /// kept and reports count at once.
    pub fn restart_reported(&mut self) {
        let on_screen = match &mut self.phase {
            Phase::Reported { on_screen, .. } => std::mem::take(on_screen),
            Phase::Newest(_) => BTreeSet::new(),
        };
        self.phase = Phase::Reported {
            stages: Stages::new(),
            on_screen,
        };
    }

    /// A topic is opened again: what was viewed or due goes, views on
    /// their way stay.
    pub fn forget_settled(&mut self) {
        self.stages_mut()
            .retain(|_, stage| matches!(stage, ViewStage::InFlight(_)));
    }

    pub fn reported(&self) -> bool {
        matches!(self.phase, Phase::Reported { .. })
    }

    /// The rows of the UI's latest report (none before one).
    pub fn on_screen(&self) -> &BTreeSet<i64> {
        static NONE: BTreeSet<i64> = BTreeSet::new();
        match &self.phase {
            Phase::Newest(_) => &NONE,
            Phase::Reported { on_screen, .. } => on_screen,
        }
    }

    /// The UI no longer shows the rows.
    pub fn leave_screen(&mut self) {
        if let Phase::Reported { on_screen, .. } = &mut self.phase {
            on_screen.clear();
        }
    }

    pub fn stage(&self, id: i64) -> Option<ViewStage> {
        self.stages().get(&id).copied()
    }

    pub fn is_viewed(&self, id: i64) -> bool {
        self.stage(id) == Some(ViewStage::Done)
    }

    /// `id` is in a `viewMessages` still on its way.
    pub fn is_viewing(&self, id: i64) -> bool {
        matches!(self.stage(id), Some(ViewStage::InFlight(_)))
    }

    /// `id` may be sent: it is not viewed and not on its way.
    pub fn is_due(&self, id: i64) -> bool {
        id > 0 && matches!(self.stage(id), None | Some(ViewStage::Due))
    }

    /// The ids to send now: the reported rows not sent yet and the rows on
    /// screen that are due again.
    pub fn due_ids(&self) -> BTreeSet<i64> {
        let mut ids: BTreeSet<i64> = self
            .stages()
            .iter()
            .filter(|(_, stage)| **stage == ViewStage::Due)
            .map(|(id, _)| *id)
            .collect();
        ids.extend(
            self.on_screen()
                .iter()
                .copied()
                .filter(|id| self.is_due(*id)),
        );
        ids
    }

    /// The UI shows `ids`: they become due.
    pub fn report(&mut self, ids: impl IntoIterator<Item = i64>) {
        let on_screen: BTreeSet<i64> = ids.into_iter().filter(|id| *id > 0).collect();
        let mut stages = match std::mem::replace(&mut self.phase, Phase::Newest(Stages::new())) {
            Phase::Newest(stages) | Phase::Reported { stages, .. } => stages,
        };
        for id in &on_screen {
            if id > &0 && matches!(stages.get(id), None | Some(ViewStage::Due)) {
                stages.insert(*id, ViewStage::Due);
            }
        }
        self.phase = Phase::Reported { stages, on_screen };
    }

    /// TDLib has these as viewed (an answer, or a live view).
    pub fn mark_done(&mut self, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            self.stages_mut().insert(id, ViewStage::Done);
        }
    }

    /// `ids` went out as `request`.
    pub fn begin(&mut self, request: RequestId, ids: impl IntoIterator<Item = i64>) {
        for id in ids {
            self.stages_mut().insert(id, ViewStage::InFlight(request));
        }
    }

    /// TDLib accepted `request`: its ids, and only its ids, are viewed.
    pub fn commit(&mut self, request: RequestId) {
        for stage in self.stages_mut().values_mut() {
            if *stage == ViewStage::InFlight(request) {
                *stage = ViewStage::Done;
            }
        }
    }

    /// TDLib refused `request` (or it was lost): its ids are due again when
    /// rows were reported (the retry re-sends them), else forgotten (the
    /// newest row is recomputed anyway).
    pub fn abort(&mut self, request: RequestId) {
        let reported = self.reported();
        self.stages_mut().retain(|_, stage| {
            if *stage != ViewStage::InFlight(request) {
                return true;
            }
            if reported {
                *stage = ViewStage::Due;
                true
            } else {
                false
            }
        });
    }

    /// `id` is due again (a new unread reaction) or gone from the window.
    pub fn forget(&mut self, id: i64) {
        self.stages_mut().remove(&id);
    }

    /// The ids TDLib accepted, for tests.
    #[cfg(test)]
    pub fn done_ids(&self) -> BTreeSet<i64> {
        self.stages()
            .iter()
            .filter(|(_, stage)| **stage == ViewStage::Done)
            .map(|(id, _)| *id)
            .collect()
    }
}

//! State of the gift management and send-gift dialogs. Lives in
//! `PaymentsState::gifts_manage`, so the poll loop and reducer own every
//! transition.

use super::{
    AvailableGift, GiftCollection, GiftLimits, GiftRecipient, PremiumGiftOption, TransferPicker,
};

/// Whether the form's request is out: the box that sent it is the one its
/// answer closes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlowAsk {
    /// The form is being filled in.
    #[default]
    Editing,
    /// The request went out; only its answer closes this form.
    Sent(crate::ids::RequestId),
}

/// The form or dialog the gifts box has open apart from a gift's panel. The
/// Upgrade and resale forms name a gift and live in that gift's panel
/// (`GiftView::Form`), so only flows that name no shown gift are here: the
/// collection forms, and the transfer picker, which is its own box with a
/// request of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftFlow {
    /// The transfer picker: a contact, or the export through Fragment.
    Transfer(TransferPicker),
    NewCollection(FlowAsk),
    RenameCollection(i32, FlowAsk),
    DeleteCollection(i32, FlowAsk),
}

/// The collection form a submit is for: sent from that form's own button,
/// never matched from the request's purpose, so a collection change sent
/// from the gifts list does not become the open form's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowForm {
    New,
    Rename(i32),
    Delete(i32),
}

impl GiftFlow {
    fn ask_mut(&mut self) -> Option<&mut FlowAsk> {
        match self {
            Self::NewCollection(ask)
            | Self::RenameCollection(_, ask)
            | Self::DeleteCollection(_, ask) => Some(ask),
            Self::Transfer(_) => None,
        }
    }

    fn ask(&self) -> Option<FlowAsk> {
        match self {
            Self::NewCollection(ask)
            | Self::RenameCollection(_, ask)
            | Self::DeleteCollection(_, ask) => Some(*ask),
            Self::Transfer(_) => None,
        }
    }

    /// Whether this is the open collection form `form` names.
    fn is_form(&self, form: FlowForm) -> bool {
        match (self, form) {
            (Self::NewCollection(_), FlowForm::New) => true,
            (Self::RenameCollection(open, _), FlowForm::Rename(id))
            | (Self::DeleteCollection(open, _), FlowForm::Delete(id)) => *open == id,
            _ => false,
        }
    }

    /// Whether the answer of `request` belongs to this form: only the form
    /// that sent it closes on it, not whichever one is open when it lands.
    pub fn answers(&self, request: Option<crate::ids::RequestId>) -> bool {
        matches!((self.ask(), request), (Some(FlowAsk::Sent(sent)), Some(request)) if sent == request)
    }
}

/// One list the send-gift dialog asks for when it opens. The dialog is
/// rebuilt for each recipient, so only the answer (or failure) of the
/// request named here lands; an earlier box's is dropped. The list is the
/// answer: it exists only in `Answered`.
#[derive(Debug, Clone, PartialEq)]
pub enum GiftListFetch<T> {
    /// Not asked (the demo, or Premium options for yourself or a channel).
    NotAsked,
    /// The request in flight.
    Asking(crate::ids::RequestId),
    /// The answer landed.
    Answered(Vec<T>),
    /// The request failed or could not be sent; the line the box shows.
    /// Apart from `SendStage`, so a list failure never overwrites a
    /// `sendGift` in flight.
    Failed(String),
}

#[allow(
    clippy::derivable_impls,
    reason = "a derive would need `T: Default`, which a list of gifts does not"
)]
impl<T> Default for GiftListFetch<T> {
    fn default() -> Self {
        Self::NotAsked
    }
}

impl<T> GiftListFetch<T> {
    /// Whether `pending` is the request this list waits for.
    pub(crate) fn asked_by(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        matches!((self, pending), (Self::Asking(id), Some(pending)) if *id == pending.id)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Asking(_))
    }

    /// The list, once answered; empty before.
    pub fn items(&self) -> &[T] {
        match self {
            Self::Answered(items) => items,
            _ => &[],
        }
    }

    /// The list is answered, with nothing in it.
    pub fn answered_empty(&self) -> bool {
        matches!(self, Self::Answered(items) if items.is_empty())
    }
}

/// What the open send-gift dialog is doing with its `sendGift`.
/// The dialog is rebuilt for each recipient, so only the answer of the
/// sending step's own request leaves it; picking another gift, closing the
/// box or sending again leaves `Done` and `Failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStep {
    /// The dialog is open and nothing is out.
    Idle,
    /// The `sendGift` in flight, its dialog open.
    Sending(crate::ids::RequestId),
    /// The `sendGift` in flight, its dialog closed: the answer is told as a
    /// notice. Opening the dialog again for the same recipient returns to
    /// `Sending`.
    SendingClosed(crate::ids::RequestId),
    /// The gift went out and its answer is in; the line the box shows.
    Done(String),
    /// The send failed; the line the box shows.
    Failed(String),
}

/// Where the send-gift dialog stands, open or not. What belongs to the open
/// dialog (the recipient, the two lists it asks for, the gift picked) lives
/// in the open stage: a closed dialog has none.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum SendStage {
    /// No dialog: nothing open, nothing out.
    #[default]
    Closed,
    /// A dialog for `recipient`, or its `sendGift` still out after it closed.
    Open {
        recipient: GiftRecipient,
        step: SendStep,
        /// `getAvailableGifts`, and its answer.
        gifts_fetch: GiftListFetch<AvailableGift>,
        /// `getPremiumGiftPaymentOptions`, and its answer.
        premium_fetch: GiftListFetch<PremiumGiftOption>,
        /// The gift picked in the list.
        selected: Option<i64>,
    },
}

/// The send-gift dialog (`GiftBox` in tdesktop).
#[derive(Debug, Clone, Default)]
pub struct SendGiftState {
    pub stage: SendStage,
}

impl SendGiftState {
    /// A dialog open for `recipient`, nothing asked yet.
    pub fn opened_for(recipient: GiftRecipient) -> Self {
        Self {
            stage: SendStage::Open {
                recipient,
                step: SendStep::Idle,
                gifts_fetch: GiftListFetch::NotAsked,
                premium_fetch: GiftListFetch::NotAsked,
                selected: None,
            },
        }
    }

    /// Who the dialog (open, or closed over its send) is for.
    pub fn recipient(&self) -> Option<&GiftRecipient> {
        match &self.stage {
            SendStage::Open { recipient, .. } => Some(recipient),
            SendStage::Closed => None,
        }
    }

    pub fn step(&self) -> Option<&SendStep> {
        match &self.stage {
            SendStage::Open { step, .. } => Some(step),
            SendStage::Closed => None,
        }
    }

    /// Moves the open stage to `next`; a closed dialog stays closed.
    pub fn set_step(&mut self, next: SendStep) {
        if let SendStage::Open { step, .. } = &mut self.stage {
            *step = next;
        }
    }

    fn gifts_fetch(&self) -> Option<&GiftListFetch<AvailableGift>> {
        match &self.stage {
            SendStage::Open { gifts_fetch, .. } => Some(gifts_fetch),
            SendStage::Closed => None,
        }
    }

    fn premium_fetch(&self) -> Option<&GiftListFetch<PremiumGiftOption>> {
        match &self.stage {
            SendStage::Open { premium_fetch, .. } => Some(premium_fetch),
            SendStage::Closed => None,
        }
    }

    /// The gift list is now `fetch`; a closed dialog has none to set.
    pub fn set_gifts_fetch(&mut self, fetch: GiftListFetch<AvailableGift>) {
        if let SendStage::Open { gifts_fetch, .. } = &mut self.stage {
            *gifts_fetch = fetch;
        }
    }

    /// The Premium options are now `fetch`; a closed dialog has none to set.
    pub fn set_premium_fetch(&mut self, fetch: GiftListFetch<PremiumGiftOption>) {
        if let SendStage::Open { premium_fetch, .. } = &mut self.stage {
            *premium_fetch = fetch;
        }
    }

    /// The gift picked (or none); a closed dialog picks nothing.
    pub fn select(&mut self, id: Option<i64>) {
        if let SendStage::Open { selected, .. } = &mut self.stage {
            *selected = id;
        }
    }

    pub fn selected(&self) -> Option<i64> {
        match &self.stage {
            SendStage::Open { selected, .. } => *selected,
            SendStage::Closed => None,
        }
    }

    /// The gifts `getAvailableGifts` listed.
    pub fn gifts(&self) -> &[AvailableGift] {
        self.gifts_fetch().map_or(&[], GiftListFetch::items)
    }

    /// The Premium options `getPremiumGiftPaymentOptions` listed.
    pub fn premium_options(&self) -> &[PremiumGiftOption] {
        self.premium_fetch().map_or(&[], GiftListFetch::items)
    }

    pub fn gifts_loading(&self) -> bool {
        self.gifts_fetch().is_some_and(GiftListFetch::is_loading)
    }

    pub fn premium_loading(&self) -> bool {
        self.premium_fetch().is_some_and(GiftListFetch::is_loading)
    }

    pub fn gifts_answered_empty(&self) -> bool {
        self.gifts_fetch()
            .is_some_and(GiftListFetch::answered_empty)
    }

    /// The dialog is open.
    pub fn is_open(&self) -> bool {
        matches!(self.step(), Some(step) if !matches!(step, SendStep::SendingClosed(_)))
    }

    /// A `sendGift` is in flight, with the dialog open or closed.
    pub fn sending(&self) -> bool {
        matches!(
            self.step(),
            Some(SendStep::Sending(_) | SendStep::SendingClosed(_))
        )
    }

    /// Whether `pending` is the `sendGift` this dialog waits for.
    pub(crate) fn waits_for(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        pending.is_some_and(|pending| {
            matches!(self.step(), Some(SendStep::Sending(id) | SendStep::SendingClosed(id)) if *id == pending.id)
        })
    }

    /// The dialog is closed: a `sendGift` in flight carries on without it.
    pub fn close(&mut self) {
        self.select(None);
        match self.step().cloned() {
            Some(SendStep::Sending(request) | SendStep::SendingClosed(request)) => {
                self.set_step(SendStep::SendingClosed(request));
            }
            _ => self.stage = SendStage::Closed,
        }
    }

    /// The dialog is shown again over the `sendGift` it closed on.
    pub fn reopen(&mut self) {
        if let Some(&SendStep::SendingClosed(request)) = self.step() {
            self.set_step(SendStep::Sending(request));
        }
    }

    /// The line after a sent gift.
    pub fn done_line(&self) -> Option<&str> {
        match self.step() {
            Some(SendStep::Done(line)) => Some(line),
            _ => None,
        }
    }

    /// The line after a failed send, gift list or Premium options list.
    pub fn error_line(&self) -> Option<&str> {
        match (self.step(), self.gifts_fetch(), self.premium_fetch()) {
            (Some(SendStep::Failed(line)), _, _)
            | (_, Some(GiftListFetch::Failed(line)), _)
            | (_, _, Some(GiftListFetch::Failed(line))) => Some(line),
            _ => None,
        }
    }

    /// A list the box shows failed: Retry asks it again.
    pub fn lists_failed(&self) -> bool {
        matches!(self.gifts_fetch(), Some(GiftListFetch::Failed(_)))
            || matches!(self.premium_fetch(), Some(GiftListFetch::Failed(_)))
    }

    /// Which of the two lists failed: (gifts, Premium options).
    pub fn failed_lists(&self) -> (bool, bool) {
        (
            matches!(self.gifts_fetch(), Some(GiftListFetch::Failed(_))),
            matches!(self.premium_fetch(), Some(GiftListFetch::Failed(_))),
        )
    }

    /// Whether `pending` is the gift list request of this dialog.
    pub(crate) fn asked_gifts(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        self.gifts_fetch().is_some_and(|fetch| fetch.asked_by(pending))
    }

    /// Whether `pending` is the Premium options request of this dialog.
    pub(crate) fn asked_premium(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        self.premium_fetch().is_some_and(|fetch| fetch.asked_by(pending))
    }

    pub fn selected_gift(&self) -> Option<&AvailableGift> {
        let id = self.selected()?;
        self.gifts().iter().find(|g| g.id == id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GiftManageState {
    pub limits: GiftLimits,
    /// The `getOption` batch behind `limits`: a failed one is asked again
    /// at the next gifts open.
    pub limits_ask: crate::state::fetch::BatchAsk,
    /// The owner's collections list, and where it stands.
    pub collections_fetch: CollectionsFetch,
    /// The collection tab that is open; 0 = all gifts.
    pub active_collection: i32,
    pub flow: Option<GiftFlow>,
    /// A one-line result of the last action ("Pinned", "Upgraded to ...").
    pub note: Option<String>,
    pub send: SendGiftState,
    /// What finished that the UI shows once: the window that hosts the
    /// gifts drains them (`take_events`); a list that closes drops them.
    pub events: Vec<GiftEvent>,
}

/// The collections list of the open gifts owner, with where it stands:
/// current, on its way under a request, out of date because a collection
/// changed (the next look asks again), or failed. A list that is on its way
/// or failed keeps the one it had in `held`, so the tabs do not blink out.
/// A change while a request is out leaves it `LoadingStale`, so that answer
/// is taken but the list is asked for once more. Only the answer of the
/// request named here settles it: an earlier owner's does not.
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionsFetch {
    /// The list is current.
    Ready(Vec<GiftCollection>),
    Loading {
        held: Vec<GiftCollection>,
        request: crate::ids::RequestId,
    },
    LoadingStale {
        held: Vec<GiftCollection>,
        request: crate::ids::RequestId,
    },
    Stale(Vec<GiftCollection>),
    /// The ask failed or could not be sent: the tabs say so and offer Retry.
    Failed(Vec<GiftCollection>),
}

impl Default for CollectionsFetch {
    fn default() -> Self {
        Self::Ready(Vec::new())
    }
}

impl CollectionsFetch {
    /// The collections shown: the current list, or the one held meanwhile.
    pub fn list(&self) -> &[GiftCollection] {
        match self {
            Self::Ready(list) | Self::Stale(list) | Self::Failed(list) => list,
            Self::Loading { held, .. } | Self::LoadingStale { held, .. } => held,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. } | Self::LoadingStale { .. })
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    fn take_list(&mut self) -> Vec<GiftCollection> {
        match std::mem::take(self) {
            Self::Ready(list) | Self::Stale(list) | Self::Failed(list) => list,
            Self::Loading { held, .. } | Self::LoadingStale { held, .. } => held,
        }
    }

    /// The ask went out as `request`.
    pub fn begin(&mut self, request: crate::ids::RequestId) {
        let held = self.take_list();
        *self = Self::Loading { held, request };
    }

    /// A collection changed: the list is asked again once the answer in
    /// flight, if any, has landed.
    pub fn mark_stale(&mut self) {
        *self = match std::mem::take(self) {
            Self::Loading { held, request } | Self::LoadingStale { held, request } => {
                Self::LoadingStale { held, request }
            }
            Self::Ready(list) | Self::Stale(list) | Self::Failed(list) => Self::Stale(list),
        };
    }

    /// The ask could not be sent: the wish stays, so the next look asks again.
    pub fn unsent(&mut self) {
        let list = self.take_list();
        *self = Self::Stale(list);
    }

    /// `request` is the ask this list waits for.
    pub fn waits_for(&self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Loading { request: asked, .. } | Self::LoadingStale { request: asked, .. } if Some(*asked) == request)
    }

    /// The answer of `request`: the list is `list`; one marked stale
    /// meanwhile stays so. Any other request's changes nothing.
    pub fn answered(&mut self, request: Option<crate::ids::RequestId>, list: Vec<GiftCollection>) {
        if !self.waits_for(request) {
            return;
        }
        *self = match self {
            Self::LoadingStale { .. } => Self::Stale(list),
            _ => Self::Ready(list),
        };
    }

    /// The error of `request`: the list held stays, marked failed (or stale
    /// when a change came meanwhile, which asks again anyway). Any other
    /// request's changes nothing.
    pub fn failed(&mut self, request: Option<crate::ids::RequestId>) {
        if !self.waits_for(request) {
            return;
        }
        let stale = matches!(self, Self::LoadingStale { .. });
        let held = self.take_list();
        *self = if stale {
            Self::Stale(held)
        } else {
            Self::Failed(held)
        };
    }
}

/// A result the UI shows once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftEvent {
    /// A gift was deleted: the "Gift Deleted" toast.
    Deleted,
    /// A finished transfer: the toast and the recipient's chat.
    Transferred(GiftTransferred),
    /// Fragment's export page from `getUpgradedGiftWithdrawalUrl`, to open.
    ExportUrl(String),
    /// The password box of "Send via Blockchain" is ready for a password
    /// (it opened, or a try was refused or failed): the field takes the
    /// keyboard.
    PasswordReady,
}

/// tdesktop `ShowGiftTransferredToast` and `showPeerHistory` after
/// `transferGift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftTransferred {
    pub gift_name: String,
    pub recipient: String,
    pub user_id: i64,
}

impl GiftManageState {
    /// The open transfer picker.
    pub fn transfer(&self) -> Option<&TransferPicker> {
        match &self.flow {
            Some(GiftFlow::Transfer(picker)) => Some(picker),
            _ => None,
        }
    }

    pub fn transfer_mut(&mut self) -> Option<&mut TransferPicker> {
        match &mut self.flow {
            Some(GiftFlow::Transfer(picker)) => Some(picker),
            _ => None,
        }
    }

    /// The open `form` sent `request`: its answer closes it. Any other
    /// open form, and a form already waiting, is left alone.
    pub fn flow_sent(&mut self, form: FlowForm, request: crate::ids::RequestId) {
        if let Some(flow) = self.flow.as_mut()
            && flow.is_form(form)
            && let Some(ask) = flow.ask_mut()
            && *ask == FlowAsk::Editing
        {
            *ask = FlowAsk::Sent(request);
        }
    }

    /// `request` was refused: its form is being filled in again.
    pub fn flow_refused(&mut self, request: Option<crate::ids::RequestId>) {
        if let Some(flow) = self.flow.as_mut()
            && flow.answers(request)
            && let Some(ask) = flow.ask_mut()
        {
            *ask = FlowAsk::Editing;
        }
    }

    /// The form that sent `request` is done; any other open form stays.
    pub fn close_flow_answered_by(&mut self, request: Option<crate::ids::RequestId>) {
        if self.flow.as_ref().is_some_and(|flow| flow.answers(request)) {
            self.flow = None;
        }
    }

    /// Take what finished since the last call.
    pub fn take_events(&mut self) -> Vec<GiftEvent> {
        std::mem::take(&mut self.events)
    }

    /// The picker's password box is idle (ready for a password): queue the
    /// event that gives its field the keyboard. Called by the code that
    /// puts the box in that stage, after every answer that returns to it.
    pub fn password_ready(&mut self) {
        let idle = self.transfer().is_some_and(|picker| {
            matches!(
                picker.stage,
                super::TransferStage::Password {
                    send: super::ExportSend::Idle,
                    ..
                }
            )
        });
        if idle {
            self.events.push(GiftEvent::PasswordReady);
        }
    }

    /// The finished transfer waiting to be shown.
    pub fn transferred(&self) -> Option<&GiftTransferred> {
        self.events.iter().find_map(|event| match event {
            GiftEvent::Transferred(done) => Some(done),
            _ => None,
        })
    }

    /// The export page waiting to be opened.
    pub fn export_url(&self) -> Option<&str> {
        self.events.iter().find_map(|event| match event {
            GiftEvent::ExportUrl(url) => Some(url.as_str()),
            _ => None,
        })
    }

    /// A deleted gift's toast is waiting.
    pub fn deleted(&self) -> bool {
        self.events.contains(&GiftEvent::Deleted)
    }

    /// The collections shown (the list held while a fetch is out or failed).
    pub fn collections(&self) -> &[GiftCollection] {
        self.collections_fetch.list()
    }

    pub fn collection(&self, id: i32) -> Option<&GiftCollection> {
        self.collections().iter().find(|c| c.id == id)
    }

    /// Forget the per-owner state when another gifts list opens.
    pub fn reset_for_owner(&mut self) {
        self.collections_fetch = CollectionsFetch::default();
        self.active_collection = 0;
        self.flow = None;
        self.note = None;
        // A result nobody took belongs to the list that was closed.
        self.events.clear();
    }
}

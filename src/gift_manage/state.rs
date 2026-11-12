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
/// request named here lands; an earlier box's is dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftListFetch {
    /// Not asked (the demo, or Premium options for yourself or a channel).
    #[default]
    NotAsked,
    /// The request in flight.
    Asking(crate::ids::RequestId),
    /// The answer landed.
    Answered,
    /// The request failed or could not be sent; the line the box shows.
    /// Apart from `SendStage`, so a list failure never overwrites a
    /// `sendGift` in flight.
    Failed(String),
}

impl GiftListFetch {
    /// Whether `pending` is the request this list waits for.
    pub(crate) fn asked_by(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        matches!((self, pending), (Self::Asking(id), Some(pending)) if *id == pending.id)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Asking(_))
    }
}

/// Where the send-gift dialog stands, open or not, with its `sendGift`.
/// The dialog is rebuilt for each recipient, so only the answer of the
/// sending stage's own request leaves it; picking another gift, closing the
/// box or sending again leaves `Sent` and `Failed`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SendStage {
    /// No dialog: nothing open, nothing out.
    #[default]
    Closed,
    /// The dialog is open and nothing is out.
    Idle,
    /// The `sendGift` in flight, its dialog open.
    Sending(crate::ids::RequestId),
    /// The `sendGift` in flight, its dialog closed: the answer is told as a
    /// notice. Opening the dialog again for the same recipient returns to
    /// `Sending`.
    SendingClosed(crate::ids::RequestId),
    /// The gift went out; the line the box shows.
    Sent(String),
    /// The send failed; the line the box shows.
    Failed(String),
}

/// The send-gift dialog (`GiftBox` in tdesktop).
#[derive(Debug, Clone, Default)]
pub struct SendGiftState {
    pub recipient: Option<GiftRecipient>,
    pub gifts: Vec<AvailableGift>,
    /// `getAvailableGifts` for `gifts`.
    pub gifts_fetch: GiftListFetch,
    pub selected: Option<i64>,
    pub premium_options: Vec<PremiumGiftOption>,
    /// `getPremiumGiftPaymentOptions` for `premium_options`.
    pub premium_fetch: GiftListFetch,
    pub stage: SendStage,
}

impl SendGiftState {
    /// The dialog is open.
    pub fn is_open(&self) -> bool {
        !matches!(self.stage, SendStage::Closed | SendStage::SendingClosed(_))
    }

    /// A `sendGift` is in flight, with the dialog open or closed.
    pub fn sending(&self) -> bool {
        matches!(
            self.stage,
            SendStage::Sending(_) | SendStage::SendingClosed(_)
        )
    }

    /// Whether `pending` is the `sendGift` this dialog waits for.
    pub(crate) fn waits_for(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        pending.is_some_and(|pending| {
            matches!(self.stage, SendStage::Sending(id) | SendStage::SendingClosed(id) if id == pending.id)
        })
    }

    /// The dialog is closed: a `sendGift` in flight carries on without it.
    pub fn close(&mut self) {
        self.selected = None;
        self.stage = match self.stage {
            SendStage::Sending(request) | SendStage::SendingClosed(request) => {
                SendStage::SendingClosed(request)
            }
            _ => SendStage::Closed,
        };
    }

    /// The dialog is shown again over the `sendGift` it closed on.
    pub fn reopen(&mut self) {
        if let SendStage::SendingClosed(request) = self.stage {
            self.stage = SendStage::Sending(request);
        }
    }

    /// The line after a sent gift.
    pub fn done_line(&self) -> Option<&str> {
        match &self.stage {
            SendStage::Sent(line) => Some(line),
            _ => None,
        }
    }

    /// The line after a failed send, gift list or Premium options list.
    pub fn error_line(&self) -> Option<&str> {
        match (&self.stage, &self.gifts_fetch, &self.premium_fetch) {
            (SendStage::Failed(line), _, _)
            | (_, GiftListFetch::Failed(line), _)
            | (_, _, GiftListFetch::Failed(line)) => Some(line),
            _ => None,
        }
    }

    /// A list the box shows failed: Retry asks it again.
    pub fn lists_failed(&self) -> bool {
        matches!(self.gifts_fetch, GiftListFetch::Failed(_))
            || matches!(self.premium_fetch, GiftListFetch::Failed(_))
    }

    /// Whether `pending` is the gift list request of this dialog.
    pub(crate) fn asked_gifts(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        self.gifts_fetch.asked_by(pending)
    }

    /// Whether `pending` is the Premium options request of this dialog.
    pub(crate) fn asked_premium(&self, pending: Option<&crate::state::PendingRequest>) -> bool {
        self.premium_fetch.asked_by(pending)
    }

    pub fn selected_gift(&self) -> Option<&AvailableGift> {
        let id = self.selected?;
        self.gifts.iter().find(|g| g.id == id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GiftManageState {
    pub limits: GiftLimits,
    /// The `getOption` batch behind `limits`: a failed one is asked again
    /// at the next gifts open.
    pub limits_ask: crate::state::fetch::BatchAsk,
    pub collections: Vec<GiftCollection>,
    /// Where the owner's collections list stands.
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

/// The collections list of the open gifts owner: current, on its way under
/// a request, or out of date because a collection changed (the next look
/// asks again). A change while a request is out leaves it
/// `LoadingStale`, so that answer is taken but the list is asked for once
/// more. Only the answer of the request named here settles it: an earlier
/// owner's does not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CollectionsFetch {
    #[default]
    Idle,
    Loading(crate::ids::RequestId),
    LoadingStale(crate::ids::RequestId),
    Stale,
}

impl CollectionsFetch {
    pub fn is_loading(self) -> bool {
        matches!(self, Self::Loading(_) | Self::LoadingStale(_))
    }

    pub fn is_stale(self) -> bool {
        self == Self::Stale
    }

    /// The ask went out as `request`.
    pub fn begin(&mut self, request: crate::ids::RequestId) {
        *self = Self::Loading(request);
    }

    /// A collection changed: the list is asked again once the answer in
    /// flight, if any, has landed.
    pub fn mark_stale(&mut self) {
        *self = match *self {
            Self::Loading(request) | Self::LoadingStale(request) => Self::LoadingStale(request),
            _ => Self::Stale,
        };
    }

    /// `request` is the ask this list waits for.
    pub fn waits_for(self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(self, Self::Loading(asked) | Self::LoadingStale(asked) if Some(asked) == request)
    }

    /// The answer (or error) of `request`: a list marked stale meanwhile
    /// stays so. Any other request's changes nothing.
    pub fn settle(&mut self, request: Option<crate::ids::RequestId>) {
        if !self.waits_for(request) {
            return;
        }
        *self = match *self {
            Self::LoadingStale(_) => Self::Stale,
            _ => Self::Idle,
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

    pub fn collection(&self, id: i32) -> Option<&GiftCollection> {
        self.collections.iter().find(|c| c.id == id)
    }

    /// Forget the per-owner state when another gifts list opens.
    pub fn reset_for_owner(&mut self) {
        self.collections.clear();
        self.collections_fetch = CollectionsFetch::Idle;
        self.active_collection = 0;
        self.flow = None;
        self.note = None;
        // A result nobody took belongs to the list that was closed.
        self.events.clear();
    }
}

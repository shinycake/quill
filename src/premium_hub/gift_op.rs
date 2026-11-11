//! What the gifts list's change in flight is, so its answer acts on the
//! right gift: it rides inside the hub's `gifts_op` stage.
#![deny(clippy::indexing_slicing)]

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftMutation {
    /// Nothing the answer needs to know (pin, upgrade, transfer, ...).
    #[default]
    Other,
    /// `deleteGift` of the gift with this id; the answer removes it.
    Delete(String),
}

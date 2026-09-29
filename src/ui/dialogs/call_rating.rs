use super::super::*;

/// Phase C2i: rating-detail draft for the call-end card. `problems` is
/// indexed by `CALL_PROBLEMS` (schema 1.8.67, `:7253`-`:7277`).
pub struct RatingDetail {
    pub(crate) stars: i32,
    pub(crate) problems: [bool; 9],
}

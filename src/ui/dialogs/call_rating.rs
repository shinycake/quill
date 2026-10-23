use quill::lang::{Key, keys};
/// Phase C2i: rating-detail draft for the call-end card. `problems` is
/// indexed by `CALL_PROBLEMS` (schema 1.8.67, `:7253`-`:7277`).
pub struct RatingDetail {
    pub(crate) stars: i32,
    pub(crate) problems: [bool; CALL_PROBLEMS.len()],
}

/// Phase C2i: the nine `CallProblem` constructors (TDLib 1.8.67,
/// `schema/td_api.tl:7253`-`:7277`) with their schema descriptions,
/// in schema order. Index-aligned with `RatingDetail::problems`.
pub const CALL_PROBLEMS: [(&str, Key); 9] = [
    ("callProblemEcho", keys::Q_LEFTOVERS_CALL_PROBLEM_ECHO),
    ("callProblemNoise", keys::Q_LEFTOVERS_CALL_PROBLEM_NOISE),
    (
        "callProblemInterruptions",
        keys::Q_LEFTOVERS_CALL_PROBLEM_INTERRUPTIONS,
    ),
    (
        "callProblemDistortedSpeech",
        keys::Q_LEFTOVERS_CALL_PROBLEM_DISTORTED_SPEECH,
    ),
    (
        "callProblemSilentLocal",
        keys::Q_LEFTOVERS_CALL_PROBLEM_SILENT_LOCAL,
    ),
    (
        "callProblemSilentRemote",
        keys::Q_LEFTOVERS_CALL_PROBLEM_SILENT_REMOTE,
    ),
    ("callProblemDropped", keys::Q_LEFTOVERS_CALL_PROBLEM_DROPPED),
    (
        "callProblemDistortedVideo",
        keys::Q_LEFTOVERS_CALL_PROBLEM_DISTORTED_VIDEO,
    ),
    (
        "callProblemPixelatedVideo",
        keys::Q_LEFTOVERS_CALL_PROBLEM_PIXELATED_VIDEO,
    ),
];

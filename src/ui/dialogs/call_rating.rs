use quill::lang::{Key, keys};
/// Phase C2i: the nine `CallProblem` constructors (TDLib 1.8.67,
/// `schema/td_api.tl:7253`-`:7277`) with their schema descriptions,
/// in schema order. Index-aligned with `RatingDraft::problems`, which lives
/// in the call's rating stage.
pub const CALL_PROBLEMS: [(&str, Key); quill::state::CALL_PROBLEM_COUNT] = [
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

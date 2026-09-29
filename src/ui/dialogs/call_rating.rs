/// Phase C2i: rating-detail draft for the call-end card. `problems` is
/// indexed by `CALL_PROBLEMS` (schema 1.8.67, `:7253`-`:7277`).
pub struct RatingDetail {
    pub(crate) stars: i32,
    pub(crate) problems: [bool; 9],
}

/// Phase C2i: the nine `CallProblem` constructors (TDLib 1.8.67,
/// `schema/td_api.tl:7253`-`:7277`) with their schema descriptions,
/// in schema order. Index-aligned with `RatingDetail::problems`.
pub const CALL_PROBLEMS: [(&str, &str); 9] = [
    ("callProblemEcho", "Echo — I heard my own voice"),
    ("callProblemNoise", "Noise — background noise"),
    (
        "callProblemInterruptions",
        "Interruptions — the other side kept disappearing",
    ),
    ("callProblemDistortedSpeech", "Distorted speech"),
    (
        "callProblemSilentLocal",
        "Silent — I couldn't hear the other side",
    ),
    (
        "callProblemSilentRemote",
        "Silent — the other side couldn't hear me",
    ),
    (
        "callProblemDropped",
        "Dropped — the call ended unexpectedly",
    ),
    ("callProblemDistortedVideo", "Distorted video"),
    ("callProblemPixelatedVideo", "Pixelated video"),
];

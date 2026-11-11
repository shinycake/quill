//! The send box: files picked for the open chat wait in a dialog over it
//! until they are sent (tdesktop `SendFilesBox`). The photos and videos
//! are laid out as the messages they will be, with the files as rows under
//! them; under those, every send option as a labelled control; then the
//! caption, which is the message field itself, and Add, Cancel and Send.
//! `dialog.rs` is the box, `tiles.rs` the previews and what each one
//! offers, `options.rs` the choices, `layout.rs` the sizes.
#![deny(clippy::indexing_slicing)]

#[cfg(feature = "demo-capture")]
mod demo;
mod dialog;
mod layout;
mod options;
mod tiles;
mod video_preview;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

pub(crate) use video_preview::VideoPreview;
pub(in crate::ui) use video_preview::discard as discard_preview;

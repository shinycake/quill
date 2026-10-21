//! The QR code box (tdesktop `FillPeerQrBox`): a peer's link, or another
//! link such as an invite or a proxy, as a QR code on a card over a
//! colored backdrop with an optional emoji pattern.

pub mod appearance;
pub mod color;
pub mod free;
pub mod link;
pub mod pattern;
pub mod session;
pub mod state;

#[cfg(test)]
mod tests;

//! The QR code box (tdesktop `FillPeerQrBox`): a peer's link, or another
//! link such as an invite or a proxy, as a QR code on a card over a
//! colored backdrop with an emoji or upgraded-gift pattern, and the
//! peer's saved share theme.

pub mod appearance;
pub mod color;
pub mod free;
pub mod gift_pattern;
pub mod gift_tiles;
pub mod layout;
pub mod link;
pub mod pattern;
pub mod requests;
pub mod session;
pub mod state;
pub mod theme;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod theme_tests;

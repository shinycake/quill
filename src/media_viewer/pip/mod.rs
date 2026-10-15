//! Picture-in-Picture's pure parts: the floating window's geometry
//! ([`panel`], tdesktop `PipPanel`) and its controls' layout and press
//! rules ([`layout`], tdesktop `Pip`). The window itself is
//! `ui::video_player::controls::pip`.

pub mod layout;
pub mod panel;

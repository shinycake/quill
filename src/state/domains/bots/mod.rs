//! The bots domain: bots: inline queries, callback buttons, commands, games and login URLs.
mod apply;
mod error;
mod owned;
mod purpose;
mod state;

pub use purpose::*;

pub use owned::*;
pub use state::*;

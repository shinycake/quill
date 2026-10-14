//! Advanced settings behind the Advanced page: the typed secret codes
//! (tdesktop `settings_codes.cpp`), the debug log mode, the test
//! environment, the experimental options and the update channel.
#![deny(clippy::indexing_slicing)]

pub mod codes;
pub mod debug_mode;
pub mod experimental;
pub mod test_environment;
pub mod update_channel;

//! Advanced settings behind the Advanced page: the typed secret codes
//! (tdesktop `settings_codes.cpp`), the debug log mode, the test
//! environment, the experimental options and hardware
//! video decoding.
#![deny(clippy::indexing_slicing)]

pub mod codes;
pub mod debug_mode;
pub mod experimental;
pub mod hardware_video;
pub mod platform_log;
pub mod test_environment;

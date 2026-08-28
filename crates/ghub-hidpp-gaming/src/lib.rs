//! HID++ features that Logitech gaming devices expose.
//!
//! Gaming mice do not implement `0x1b04`, the reprogrammable-controls feature
//! that productivity mice use. They implement `0x8100` onboard profiles and
//! `0x8110` button spy instead. This crate decodes those.

#![forbid(unsafe_code)]

pub mod onboard_profiles;

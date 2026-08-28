//! The OpenHub hardware model table.
//!
//! OpenLogi discovers a mouse's remappable buttons over HID++ feature
//! `0x1b04`. Gaming mice do not implement that feature — their buttons live
//! behind `0x8100` onboard profiles — so a G703 appears there with no buttons
//! at all. This crate is the answer: a static table that says what buttons a
//! model has and what each one emits.
//!
//! It is pure data. No I/O, no async, no platform code.

#![forbid(unsafe_code)]

mod catalog;
mod slot;

pub use catalog::{
    DeviceModel, G703_HERO, MODELS, model_for_hidpp_id, model_for_product_ids, model_for_usb_id,
};
pub use slot::{ButtonSlot, SlotId, codes};

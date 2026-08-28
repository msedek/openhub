//! OpenHub's macro engine.
//!
//! A macro is a sequence of press/release steps plus one of G HUB's three
//! repeat modes. The mode that matters is `WhileHeld`: hold a mouse button and
//! the sequence repeats at a fixed interval until the button comes up. Nothing
//! on Linux reproduces that today — libratbag's macros are one-shot and the
//! mouse's own firmware has no repeat mode — which is why this crate exists.
//!
//! The types are pure `std` data: no I/O, no async, no platform code.

#![forbid(unsafe_code)]

#[cfg(feature = "exec")]
mod executor;
mod model;
#[cfg(feature = "exec")]
mod timer;

#[cfg(feature = "exec")]
pub use executor::{Executor, RunHandle, Sink};
pub use model::{Macro, MacroId, RepeatMode, Step};

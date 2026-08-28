//! Button slots: the identity of a physical button on a device model.
//!
//! A slot is named the way Logitech's own software names it — `G1`, `G2`, and
//! so on — because that is the vocabulary the user already has. What the slot
//! *is* on a given model, and what evdev code it emits, comes from the model
//! table; nothing here is device-specific.

/// A button position on a gaming device, in Logitech's `G`-numbered naming.
///
/// The numbering is per model: `G4` is the rear side button on a G703 and
/// something else entirely on a G502. Only [`crate::DeviceModel`] gives a slot
/// its meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SlotId {
    /// Slot 1 — the primary button on every model seen so far.
    G1,
    /// Slot 2 — the secondary button on every model seen so far.
    G2,
    /// Slot 3.
    G3,
    /// Slot 4.
    G4,
    /// Slot 5.
    G5,
    /// Slot 6.
    G6,
    /// Slot 7.
    G7,
    /// Slot 8.
    G8,
    /// Slot 9.
    G9,
    /// Slot 10.
    G10,
    /// Slot 11.
    G11,
}

/// One physical button of a device model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonSlot {
    /// Which slot this is, in the device's own numbering.
    pub id: SlotId,
    /// The Linux input event code the button emits, from
    /// `linux/input-event-codes.h`. See [`codes`].
    pub evdev_code: u16,
    /// Where the button physically is, in words, for the UI.
    pub label: &'static str,
}

/// The `BTN_*` codes from `linux/input-event-codes.h` that mice emit.
///
/// Spelled out here rather than pulled from a crate: the table is short, it
/// never changes, and the model catalog reads better naming them than writing
/// bare integers.
pub mod codes {
    /// `BTN_LEFT`.
    pub const BTN_LEFT: u16 = 272;
    /// `BTN_RIGHT`.
    pub const BTN_RIGHT: u16 = 273;
    /// `BTN_MIDDLE`.
    pub const BTN_MIDDLE: u16 = 274;
    /// `BTN_SIDE`. The system reads this as "back", so pressing it navigates
    /// backwards in most applications.
    pub const BTN_SIDE: u16 = 275;
    /// `BTN_EXTRA`.
    pub const BTN_EXTRA: u16 = 276;
    /// `BTN_FORWARD`.
    pub const BTN_FORWARD: u16 = 277;
    /// `BTN_BACK`.
    pub const BTN_BACK: u16 = 278;
    /// `BTN_TASK`.
    pub const BTN_TASK: u16 = 279;
}

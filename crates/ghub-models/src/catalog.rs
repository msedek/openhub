//! The device model table.
//!
//! Supporting another gaming mouse is adding a `const` here and listing it in
//! [`MODELS`] — not writing code. Every entry must be verifiable: the evdev
//! codes come from pressing the physical buttons and reading the event node,
//! never from a datasheet or a guess.
//!
//! Only the G703 is present, because it is the only device this project has
//! been able to verify against real hardware.

use crate::slot::{ButtonSlot, SlotId, codes};

/// A gaming device this build knows how to drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceModel {
    /// Stable identifier, also the directory name under `design/devices/`.
    pub id: &'static str,
    /// The name to show a user.
    pub display_name: &'static str,
    /// The HID++ model id, which is the WPID the receiver reports.
    pub hidpp_model_id: u16,
    /// The USB product id the device enumerates as over the cable.
    pub usb_product_id: u16,
    /// Every physical button, in slot order.
    pub slots: &'static [ButtonSlot],
    /// Minimum and maximum sensor DPI, inclusive.
    pub dpi_range: (u32, u32),
    /// How many profiles the device's onboard memory holds.
    pub onboard_profile_count: u8,
}

/// Logitech G703 LIGHTSPEED HERO.
///
/// Verified against the physical device: six buttons, whose evdev codes were
/// captured by pressing each one and reading `/dev/input/event*`; a DPI range
/// of 100–25600 reported over HID++ `0x2201`, which is far finer than the five
/// presets libratbag exposes; and five onboard profile slots.
pub const G703_HERO: DeviceModel = DeviceModel {
    id: "g703_hero",
    display_name: "G703 LIGHTSPEED HERO",
    hidpp_model_id: 0x4086,
    usb_product_id: 0xc090,
    slots: &[
        ButtonSlot {
            id: SlotId::G1,
            evdev_code: codes::BTN_LEFT,
            label: "Left click",
        },
        ButtonSlot {
            id: SlotId::G2,
            evdev_code: codes::BTN_RIGHT,
            label: "Right click",
        },
        ButtonSlot {
            id: SlotId::G3,
            evdev_code: codes::BTN_MIDDLE,
            label: "Wheel click",
        },
        ButtonSlot {
            id: SlotId::G4,
            evdev_code: codes::BTN_SIDE,
            label: "Rear side button",
        },
        ButtonSlot {
            id: SlotId::G5,
            evdev_code: codes::BTN_EXTRA,
            label: "Front side button",
        },
        ButtonSlot {
            id: SlotId::G6,
            evdev_code: codes::BTN_TASK,
            label: "Behind the wheel",
        },
    ],
    dpi_range: (100, 25600),
    onboard_profile_count: 5,
};

/// Every model in this build.
pub const MODELS: &[&DeviceModel] = &[&G703_HERO];

/// Finds the model matching a HID++ model id (the receiver's WPID).
#[must_use]
pub fn model_for_hidpp_id(model_id: u16) -> Option<&'static DeviceModel> {
    MODELS
        .iter()
        .copied()
        .find(|m| m.hidpp_model_id == model_id)
}

/// Finds the model matching a USB product id (the wired enumeration).
#[must_use]
pub fn model_for_usb_id(product_id: u16) -> Option<&'static DeviceModel> {
    MODELS
        .iter()
        .copied()
        .find(|m| m.usb_product_id == product_id)
}

#[cfg(test)]
mod tests {
    use crate::{SlotId, model_for_hidpp_id, model_for_usb_id};

    /// The G703 reaches the host two ways: `4086` is its wireless WPID through
    /// the Lightspeed receiver, `c090` its USB product id on the cable. Both
    /// must resolve to the same model, or plugging the cable in would make the
    /// mouse look like a different device.
    #[test]
    fn g703_resolves_from_both_transports() {
        let wireless = model_for_hidpp_id(0x4086).expect("wireless id is known");
        let wired = model_for_usb_id(0xc090).expect("wired id is known");

        assert_eq!(wireless.id, "g703_hero");
        assert_eq!(wired.id, wireless.id);
    }

    /// Captured from the physical device, three presses per button. See the
    /// design spec §3.2.
    #[test]
    fn g703_slots_carry_the_captured_evdev_codes() {
        let model = model_for_hidpp_id(0x4086).unwrap();
        let codes: Vec<(SlotId, u16)> = model.slots.iter().map(|s| (s.id, s.evdev_code)).collect();

        assert_eq!(
            codes,
            vec![
                (SlotId::G1, 272), // BTN_LEFT
                (SlotId::G2, 273), // BTN_RIGHT
                (SlotId::G3, 274), // BTN_MIDDLE
                (SlotId::G4, 275), // BTN_SIDE, rear side button
                (SlotId::G5, 276), // BTN_EXTRA, front side button
                (SlotId::G6, 279), // BTN_TASK, behind the wheel
            ]
        );
    }

    #[test]
    fn an_unknown_id_resolves_to_nothing() {
        assert!(model_for_hidpp_id(0xffff).is_none());
        assert!(model_for_usb_id(0xffff).is_none());
    }
}

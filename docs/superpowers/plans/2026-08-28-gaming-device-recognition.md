# Gaming Device Recognition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a Logitech G-series gaming mouse report its buttons to OpenHub, so that a G703 stops appearing as a device with zero remappable controls.

**Architecture:** Two new crates plus one wiring change. `ghub-hidpp-gaming` implements HID++ feature `0x8100` (Onboard Profiles) on top of the vendored `hidpp` crate, following that crate's established `FeatureEndpoint` pattern. `ghub-models` is a pure data table: for each supported device model, its button slots and the evdev code each slot emits. The device layer then reports buttons for a device that exposes `0x8100`, using the model table rather than HID++ feature `0x1b04`, which gaming mice do not implement.

**Tech Stack:** Rust 2024 edition, MSRV 1.98. `openlogi-hidpp` (vendored fork of `hidpp`, lib name `hidpp`), `openlogi-hidpp-derive` for the `Feature` derive macro, `bitflags`, `serde`. Tests are `std` unit tests over byte payloads — no hardware in the test suite.

**Spec:** [`docs/superpowers/specs/2026-08-27-openhub-design.md`](../specs/2026-08-27-openhub-design.md)

## Global Constraints

- **Language.** Every file, comment, doc string, commit message and PR body in this repository is written in **English**. No exceptions.
- **Edition 2024, MSRV 1.98.** Set `rust-version.workspace = true` on new crates.
- **Workspace lints.** New crates set `[lints] workspace = true`. The workspace lint table denies a lot; expect `missing_docs` to apply. Every public item needs a doc comment.
- **No hardware in tests.** Parsing logic is extracted into free functions that take byte slices and are unit-tested. The async I/O wrapper around them is not unit-tested.
- **`hidpp` is the lib name** of the `openlogi-hidpp` crate. Depend on it as `hidpp = { workspace = true }`.
- **Function ids are 4-bit.** `FeatureEndpoint::call` asserts `function <= 0x0f` in debug builds.
- **Verification.** `cargo test -p <crate>` and `cargo clippy -p <crate> --all-targets -- -D warnings` for each crate touched. Full-workspace checks need system libraries that are not installed on the development machine (see Task 0).
- **Never claim a check passed without running it.** If a command cannot run, say so by name.

## Prerequisites and their status

| Requirement | Status |
|---|---|
| Rust toolchain 1.98 | Installed at `~/.cargo/bin` |
| `libudev-dev clang libfontconfig-dev libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev` | **Missing.** Task 0 |
| Reference hardware (G703, receiver + cable) | Present |

## File Structure

| File | Responsibility |
|---|---|
| `crates/ghub-hidpp-gaming/Cargo.toml` | New crate manifest |
| `crates/ghub-hidpp-gaming/src/lib.rs` | Crate root; re-exports `onboard_profiles` |
| `crates/ghub-hidpp-gaming/src/onboard_profiles.rs` | The `0x8100` feature: async wrapper plus the pure parsers it delegates to |
| `crates/ghub-models/Cargo.toml` | New crate manifest |
| `crates/ghub-models/src/lib.rs` | Crate root; the `DeviceModel` lookup |
| `crates/ghub-models/src/slot.rs` | `SlotId`, `ButtonSlot`, evdev code constants |
| `crates/ghub-models/src/catalog.rs` | The model table. One `const` per device; the G703 is the only entry |
| `crates/openlogi-cli/src/diag.rs` | Add the `onboard` subcommand (modify) |
| `Cargo.toml` | Register both crates as workspace members (modify) |

## Task 0: Install the build prerequisites

**Files:** none — this is an environment task.

This blocks every later task that compiles anything outside `openlogi-core`. It needs a sudo password, so a human runs it.

- [ ] **Step 1: Install the system libraries**

```bash
sudo apt install -y libudev-dev clang libfontconfig-dev libwayland-dev \
    libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev pkg-config
```

- [ ] **Step 2: Verify the workspace compiles**

Run: `cargo check --workspace`
Expected: finishes without error. If `openlogi-desktop` fails on a missing header, a library above is still absent — read the error, install it, rerun.

- [ ] **Step 3: Verify the open pull request**

PR #1 (`feat/own-device-art`) changed ten files under `crates/openlogi-desktop` that have never been compiled. Now they can be.

Run: `git checkout feat/own-device-art && cargo clippy -p openlogi-desktop --all-targets -- -D warnings && cargo test -p openlogi-desktop`
Expected: both pass. If they do not, fix the failures on that branch and push before continuing; do not merge a branch that has never compiled.

---

## Task 1: The model table

**Files:**
- Create: `crates/ghub-models/Cargo.toml`
- Create: `crates/ghub-models/src/lib.rs`
- Create: `crates/ghub-models/src/slot.rs`
- Create: `crates/ghub-models/src/catalog.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `ghub_models::SlotId` — a `Copy` enum: `G1 G2 G3 G4 G5 G6 G7 G8 G9 G10 G11`.
  - `ghub_models::ButtonSlot { pub id: SlotId, pub evdev_code: u16, pub label: &'static str }`
  - `ghub_models::DeviceModel { pub id: &'static str, pub display_name: &'static str, pub hidpp_model_id: u16, pub usb_product_id: u16, pub slots: &'static [ButtonSlot], pub dpi_range: (u32, u32), pub onboard_profile_count: u8 }`
  - `ghub_models::model_for_hidpp_id(model_id: u16) -> Option<&'static DeviceModel>`
  - `ghub_models::model_for_usb_id(product_id: u16) -> Option<&'static DeviceModel>`

The evdev codes below are ground truth: they were captured from the physical G703 in this repository's design session, three presses per button, and are recorded in the spec at §3.2. Do not change them.

- [ ] **Step 1: Write the failing test**

Create `crates/ghub-models/src/catalog.rs` with only the test module:

```rust
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
        let codes: Vec<(SlotId, u16)> =
            model.slots.iter().map(|s| (s.id, s.evdev_code)).collect();

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
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p ghub-models`
Expected: FAIL — the package does not exist yet.

- [ ] **Step 3: Create the crate manifest**

`crates/ghub-models/Cargo.toml`:

```toml
[package]
name = "ghub-models"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Hardware model table for OpenHub: button slots, evdev codes and capabilities per gaming device"

[lints]
workspace = true

[dependencies]
serde = { workspace = true, optional = true }

[features]
default = []
serde = ["dep:serde"]
```

Add `"crates/ghub-models",` to the `members` list in the workspace `Cargo.toml`.

- [ ] **Step 4: Write the slot types**

`crates/ghub-models/src/slot.rs`:

```rust
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
```

- [ ] **Step 5: Write the catalog**

Prepend to `crates/ghub-models/src/catalog.rs`, above the test module written in Step 1:

```rust
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
        ButtonSlot { id: SlotId::G1, evdev_code: codes::BTN_LEFT, label: "Left click" },
        ButtonSlot { id: SlotId::G2, evdev_code: codes::BTN_RIGHT, label: "Right click" },
        ButtonSlot { id: SlotId::G3, evdev_code: codes::BTN_MIDDLE, label: "Wheel click" },
        ButtonSlot { id: SlotId::G4, evdev_code: codes::BTN_SIDE, label: "Rear side button" },
        ButtonSlot { id: SlotId::G5, evdev_code: codes::BTN_EXTRA, label: "Front side button" },
        ButtonSlot { id: SlotId::G6, evdev_code: codes::BTN_TASK, label: "Behind the wheel" },
    ],
    dpi_range: (100, 25600),
    onboard_profile_count: 5,
};

/// Every model in this build.
pub const MODELS: &[&DeviceModel] = &[&G703_HERO];

/// Finds the model matching a HID++ model id (the receiver's WPID).
#[must_use]
pub fn model_for_hidpp_id(model_id: u16) -> Option<&'static DeviceModel> {
    MODELS.iter().copied().find(|m| m.hidpp_model_id == model_id)
}

/// Finds the model matching a USB product id (the wired enumeration).
#[must_use]
pub fn model_for_usb_id(product_id: u16) -> Option<&'static DeviceModel> {
    MODELS.iter().copied().find(|m| m.usb_product_id == product_id)
}
```

- [ ] **Step 6: Write the crate root**

`crates/ghub-models/src/lib.rs`:

```rust
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

pub use catalog::{DeviceModel, MODELS, G703_HERO, model_for_hidpp_id, model_for_usb_id};
pub use slot::{ButtonSlot, SlotId, codes};
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p ghub-models`
Expected: 3 tests pass.

- [ ] **Step 8: Lint**

Run: `cargo clippy -p ghub-models --all-targets -- -D warnings`
Expected: clean. If `missing_docs` fires, the workspace lint table wants a doc comment on the item it names — add it rather than allowing the lint.

- [ ] **Step 9: Commit**

```bash
git add crates/ghub-models Cargo.toml
git commit -m "feat(models): add the hardware model table with the verified G703

OpenLogi finds a mouse's buttons through HID++ 0x1b04, which gaming mice do not
implement, so a G703 shows up with none. This table is the replacement: what
buttons a model has, and the evdev code each one emits.

The G703's six codes were captured from the physical device rather than read
off a datasheet, three presses per button, which also settles which of the two
side buttons is which — a question a prior note had left open."
```

---

## Task 2: Parse the `0x8100` device-info payload

**Files:**
- Create: `crates/ghub-hidpp-gaming/Cargo.toml`
- Create: `crates/ghub-hidpp-gaming/src/lib.rs`
- Create: `crates/ghub-hidpp-gaming/src/onboard_profiles.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `hidpp::feature::FeatureEndpoint` (crate-internal in `openlogi-hidpp`; see Step 3 for what that means for this crate's shape), `openlogi_hidpp_derive::Feature`.
- Produces:
  - `ghub_hidpp_gaming::onboard_profiles::OnboardProfilesInfo { pub memory_model_id: u8, pub profile_format_id: u8, pub macro_format_id: u8, pub profile_count: u8, pub profile_count_oob: u8, pub button_count: u8, pub sector_count: u8, pub sector_size: u16, pub mechanical_layout: u8, pub various_info: u8 }`
  - `ghub_hidpp_gaming::onboard_profiles::parse_info(payload: &[u8]) -> Result<OnboardProfilesInfo, ParseError>`
  - `ghub_hidpp_gaming::onboard_profiles::DeviceMode { Onboard, Host }` with `DeviceMode::from_wire(u8) -> Option<DeviceMode>` and `DeviceMode::to_wire(self) -> u8`
  - `ghub_hidpp_gaming::onboard_profiles::ParseError` (an error enum implementing `std::error::Error`)

`getOnboardProfilesInfo` is function 0 of feature `0x8100`. Its response is **eleven** bytes — ten fields, of which `sector_size` takes two — in this order: memory model id, profile format id, macro format id, profile count, profile count out-of-box, button count, sector count, sector size (two bytes, big endian), mechanical layout, various info. Device mode is function 2 and returns one byte: `0x01` means the device runs from its own memory, `0x02` means the host drives it.

- [ ] **Step 1: Write the failing tests**

Create `crates/ghub-hidpp-gaming/src/onboard_profiles.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::{DeviceMode, ParseError, parse_info};

    /// A payload shaped like what a G703 returns: five profiles, six buttons,
    /// sixteen sectors of 256 bytes.
    #[test]
    fn parses_a_full_info_payload() {
        let payload = [0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x01, 0x00, 0x04, 0x00];

        let info = parse_info(&payload).unwrap();

        assert_eq!(info.memory_model_id, 0x01);
        assert_eq!(info.profile_format_id, 0x03);
        assert_eq!(info.macro_format_id, 0x01);
        assert_eq!(info.profile_count, 5);
        assert_eq!(info.profile_count_oob, 1);
        assert_eq!(info.button_count, 6);
        assert_eq!(info.sector_count, 0x10);
        assert_eq!(info.sector_size, 0x0100);
        assert_eq!(info.mechanical_layout, 0x00);
        assert_eq!(info.various_info, 0x04);
    }

    /// The sector size is the only multi-byte field, and HID++ is big endian.
    /// Getting this backwards would read 256 as 1, which is the kind of bug
    /// that only shows up when something writes to onboard memory later.
    #[test]
    fn reads_sector_size_big_endian() {
        let payload = [0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x02, 0x00, 0x04, 0x00];

        assert_eq!(parse_info(&payload).unwrap().sector_size, 512);
    }

    /// A short payload means the device answered something this code does not
    /// understand. Truncating silently would invent a device with zero buttons.
    #[test]
    fn rejects_a_short_payload() {
        let payload = [0x01, 0x03, 0x01];

        assert!(matches!(
            parse_info(&payload),
            Err(ParseError::ShortPayload { expected: 10, got: 3 })
        ));
    }

    /// Responses arrive in a fixed-size report, so trailing padding is normal
    /// and must not be treated as an error.
    #[test]
    fn ignores_trailing_padding() {
        let payload = [0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x01, 0x00, 0x04, 0, 0, 0, 0];

        assert_eq!(parse_info(&payload).unwrap().button_count, 6);
    }

    #[test]
    fn round_trips_device_mode() {
        assert_eq!(DeviceMode::from_wire(0x01), Some(DeviceMode::Onboard));
        assert_eq!(DeviceMode::from_wire(0x02), Some(DeviceMode::Host));
        assert_eq!(DeviceMode::from_wire(0x00), None);
        assert_eq!(DeviceMode::Onboard.to_wire(), 0x01);
        assert_eq!(DeviceMode::Host.to_wire(), 0x02);
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test -p ghub-hidpp-gaming`
Expected: FAIL — the package does not exist.

- [ ] **Step 3: Create the crate manifest**

`crates/ghub-hidpp-gaming/Cargo.toml`:

```toml
[package]
name = "ghub-hidpp-gaming"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "HID++ features that Logitech gaming devices expose: onboard profiles and button spy"

[lints]
workspace = true

[dependencies]
# Deliberately empty. This crate must NOT depend on `hidpp`: Task 3 makes
# `openlogi-hidpp` depend on this crate, so a dependency the other way would
# close a cycle and cargo would reject the workspace. Nothing here needs it —
# these are pure byte parsers.
```

Add `"crates/ghub-hidpp-gaming",` to the workspace `members`.

**A decision you must make and report.** `FeatureEndpoint` is `pub(crate)` inside `openlogi-hidpp`, so a separate crate cannot construct one. Two ways out:

1. Implement the feature *inside* `crates/openlogi-hidpp/src/feature/onboard_profiles.rs`, next to its siblings, and keep `ghub-hidpp-gaming` for the parts that do not need the endpoint. This is the smaller change and matches how every other feature in that crate is written.
2. Widen `FeatureEndpoint`'s visibility so an external crate can implement features.

**Take option 1.** The vendored crate is ours to edit — this fork has no upstream to merge with — and scattering one feature's implementation across two crates to avoid touching it would be the worse trade. Put the async feature struct in `openlogi-hidpp`, and keep `ghub-hidpp-gaming` as the home for the pure parsers and the gaming-specific types the rest of the app consumes. Say in your report that you did this.

- [ ] **Step 4: Write the parsers**

Prepend to `crates/ghub-hidpp-gaming/src/onboard_profiles.rs`, above the tests:

```rust
//! Parsing for HID++ feature `0x8100`, Onboard Profiles.
//!
//! This is where a gaming mouse keeps what OpenLogi expects to find behind
//! `0x1b04`: its button map, DPI presets, report rate and lighting, in a bank
//! of profiles held in the device's own memory.
//!
//! Only the decoding lives here, as free functions over byte slices, so it can
//! be tested without a device. The async wrapper that actually talks to the
//! hardware is `hidpp::feature::onboard_profiles`.

use std::fmt;

/// What a device reports about its onboard memory layout.
///
/// Returned by function 0, `getOnboardProfilesInfo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnboardProfilesInfo {
    /// Which memory model the device uses. `0x01` is the only one seen.
    pub memory_model_id: u8,
    /// Version of the profile record layout. Decides how a profile's bytes are
    /// read, so writing to memory without checking it would corrupt the device.
    pub profile_format_id: u8,
    /// Version of the macro record layout.
    pub macro_format_id: u8,
    /// How many profiles the device holds.
    pub profile_count: u8,
    /// How many profiles ship configured out of the box.
    pub profile_count_oob: u8,
    /// How many physical buttons the device reports.
    pub button_count: u8,
    /// How many memory sectors exist.
    pub sector_count: u8,
    /// Size of one sector in bytes.
    pub sector_size: u16,
    /// Mechanical layout flags, meaningful for keyboards.
    pub mechanical_layout: u8,
    /// Device-kind flags.
    pub various_info: u8,
}

/// Whether the device drives itself or lets the host drive it.
///
/// A mouse in [`DeviceMode::Onboard`] applies its own stored profile. In
/// [`DeviceMode::Host`] it defers to software — which is the mode a running
/// agent wants, and the mode G HUB switches devices into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceMode {
    /// The device applies its own stored profile.
    Onboard,
    /// The host drives the device.
    Host,
}

impl DeviceMode {
    /// Decodes the wire value, or `None` if the device reported something this
    /// code does not know.
    #[must_use]
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Onboard),
            0x02 => Some(Self::Host),
            _ => None,
        }
    }

    /// The wire value for this mode.
    #[must_use]
    pub fn to_wire(self) -> u8 {
        match self {
            Self::Onboard => 0x01,
            Self::Host => 0x02,
        }
    }
}

/// A response that could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The response was shorter than the field it had to contain.
    ShortPayload {
        /// Bytes the layout requires.
        expected: usize,
        /// Bytes that arrived.
        got: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortPayload { expected, got } => {
                write!(f, "payload too short: expected {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Bytes `getOnboardProfilesInfo` must return.
const INFO_LEN: usize = 10;

/// Decodes a `getOnboardProfilesInfo` response.
///
/// Trailing bytes are ignored: responses arrive in a fixed-size report, so
/// padding after the last field is normal.
///
/// # Errors
///
/// [`ParseError::ShortPayload`] when fewer than ten bytes arrived.
pub fn parse_info(payload: &[u8]) -> Result<OnboardProfilesInfo, ParseError> {
    if payload.len() < INFO_LEN {
        return Err(ParseError::ShortPayload {
            expected: INFO_LEN,
            got: payload.len(),
        });
    }

    Ok(OnboardProfilesInfo {
        memory_model_id: payload[0],
        profile_format_id: payload[1],
        macro_format_id: payload[2],
        profile_count: payload[3],
        profile_count_oob: payload[4],
        button_count: payload[5],
        sector_count: payload[6],
        sector_size: u16::from_be_bytes([payload[7], payload[8]]),
        mechanical_layout: payload[9],
        various_info: payload.get(10).copied().unwrap_or(0),
    })
}
```

- [ ] **Step 5: Write the crate root**

`crates/ghub-hidpp-gaming/src/lib.rs`:

```rust
//! HID++ features that Logitech gaming devices expose.
//!
//! Gaming mice do not implement `0x1b04`, the reprogrammable-controls feature
//! that productivity mice use. They implement `0x8100` onboard profiles and
//! `0x8110` button spy instead. This crate decodes those.

#![forbid(unsafe_code)]

pub mod onboard_profiles;
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p ghub-hidpp-gaming`
Expected: 5 tests pass.

- [ ] **Step 7: Lint**

Run: `cargo clippy -p ghub-hidpp-gaming --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/ghub-hidpp-gaming Cargo.toml
git commit -m "feat(hidpp): decode the 0x8100 onboard-profiles info payload

Feature 0x8100 is where a gaming mouse keeps its button map, DPI presets,
report rate and lighting. A G703 exposes it and does not expose 0x1b04, so this
is the entry requirement for reading anything off one.

Only the decoding, as free functions over byte slices, so it is testable
without hardware. The async wrapper follows."
```

---

## Task 3: The async `0x8100` feature

> **Corrections carried forward from Task 2, which is already done.** Task 2's
> agent found two errors in this plan and verified the fix against libratbag's
> `src/hidpp20.h` rather than guessing:
>
> 1. The info payload is **eleven** bytes, not ten — `sector_size` is two of
>    them. `INFO_LEN` is 11 in the merged code.
> 2. **`ghub-hidpp-gaming` must never depend on `hidpp`.** This task makes
>    `openlogi-hidpp` depend on `ghub-hidpp-gaming`; a dependency the other way
>    is a cycle and cargo rejects the workspace. Do not add it back.
>
> The same source confirms this task's constants: functions 0–4 sit at
> addresses `0x00`, `0x10`, `0x20`, `0x30`, `0x40`, and the device modes are
> `0x01` onboard, `0x02` host.


**Files:**
- Create: `crates/openlogi-hidpp/src/feature/onboard_profiles.rs`
- Modify: `crates/openlogi-hidpp/src/feature.rs` (add `pub mod onboard_profiles;`)
- Modify: `crates/openlogi-hidpp/src/feature/registry.rs:260`
- Modify: `crates/openlogi-hidpp/Cargo.toml` (depend on `ghub-hidpp-gaming`)

**Interfaces:**
- Consumes: `ghub_hidpp_gaming::onboard_profiles::{OnboardProfilesInfo, DeviceMode, parse_info}` from Task 2; `crate::feature::FeatureEndpoint`; `openlogi_hidpp_derive::Feature`.
- Produces: `hidpp::feature::onboard_profiles::OnboardProfilesFeature`, with async methods `get_info() -> Result<OnboardProfilesInfo, Hidpp20Error>`, `get_device_mode() -> Result<Option<DeviceMode>, Hidpp20Error>`, `set_device_mode(DeviceMode) -> Result<(), Hidpp20Error>`, `get_current_profile() -> Result<u8, Hidpp20Error>`, `set_current_profile(u8) -> Result<(), Hidpp20Error>`.

Function ids on `0x8100`: 0 `getOnboardProfilesInfo`, 1 `setOnboardMode`, 2 `getOnboardMode`, 3 `setCurrentProfile`, 4 `getCurrentProfile`. Read `crates/openlogi-hidpp/src/feature/report_rate.rs` first — it is the shortest complete example of this crate's feature pattern and this file follows it exactly.

- [ ] **Step 1: Write the feature**

`crates/openlogi-hidpp/src/feature/onboard_profiles.rs`:

```rust
//! Implements the `OnboardProfiles` feature (ID `0x8100`).
//!
//! Gaming devices keep their button map, DPI presets, report rate and lighting
//! in a bank of profiles in their own memory, reached through this feature.
//! Productivity mice expose `0x1b04` instead and never implement this one.
//!
//! Decoding lives in [`ghub_hidpp_gaming::onboard_profiles`] so it can be
//! tested without a device; this file is only the transport.

use ghub_hidpp_gaming::onboard_profiles::{DeviceMode, OnboardProfilesInfo, parse_info};
use openlogi_hidpp_derive::Feature;

use crate::{feature::FeatureEndpoint, protocol::v20::Hidpp20Error};

/// Implements the `OnboardProfiles` / `0x8100` feature.
#[derive(Clone, Feature)]
#[creatable(id = 0x8100, version = 0)]
pub struct OnboardProfilesFeature {
    /// The endpoint this feature talks to.
    endpoint: FeatureEndpoint,
}

impl OnboardProfilesFeature {
    /// Reads the device's onboard memory layout.
    ///
    /// # Errors
    ///
    /// Propagates the HID++ error, or [`Hidpp20Error::UnsupportedResponse`]
    /// when the payload is too short to decode.
    pub async fn get_info(&self) -> Result<OnboardProfilesInfo, Hidpp20Error> {
        let payload = self.endpoint.call(0, [0; 3]).await?.extend_payload();
        parse_info(&payload).map_err(|_| Hidpp20Error::UnsupportedResponse)
    }

    /// Reads whether the device drives itself or defers to the host.
    ///
    /// `Ok(None)` means the device reported a mode this code does not know,
    /// which is worth surfacing rather than guessing at.
    ///
    /// # Errors
    ///
    /// Propagates the HID++ error.
    pub async fn get_device_mode(&self) -> Result<Option<DeviceMode>, Hidpp20Error> {
        let payload = self.endpoint.call(2, [0; 3]).await?.extend_payload();
        Ok(DeviceMode::from_wire(payload[0]))
    }

    /// Switches the device between running its own profile and letting the
    /// host drive it.
    ///
    /// # Errors
    ///
    /// Propagates the HID++ error.
    pub async fn set_device_mode(&self, mode: DeviceMode) -> Result<(), Hidpp20Error> {
        self.endpoint.call(1, [mode.to_wire(), 0, 0]).await?;
        Ok(())
    }

    /// Reads which onboard profile is active.
    ///
    /// # Errors
    ///
    /// Propagates the HID++ error.
    pub async fn get_current_profile(&self) -> Result<u8, Hidpp20Error> {
        Ok(self.endpoint.call(4, [0; 3]).await?.extend_payload()[1])
    }

    /// Activates an onboard profile. Profile ids are one-based on the wire.
    ///
    /// # Errors
    ///
    /// Propagates the HID++ error; devices reject an out-of-range id.
    pub async fn set_current_profile(&self, profile: u8) -> Result<(), Hidpp20Error> {
        self.endpoint.call(3, [0, profile, 0]).await?;
        Ok(())
    }
}
```

`Hidpp20Error` is defined at `crates/openlogi-hidpp/src/protocol/v20.rs:194` and its variants are `Channel`, `Feature` and `UnsupportedResponse`. Do not add a variant for this — an undecodable payload is a device saying something this build does not understand, which is what `UnsupportedResponse` already means.

- [ ] **Step 2: Register the module and the feature**

In `crates/openlogi-hidpp/src/feature.rs`, add `pub mod onboard_profiles;` to the module list, keeping alphabetical order.

In `crates/openlogi-hidpp/src/feature/registry.rs`, line 260 currently reads:

```rust
    0x8100 "OnboardProfiles",
```

Change it to:

```rust
    0x8100 "OnboardProfiles" => OnboardProfilesFeature,
```

and add `OnboardProfilesFeature` to that file's `use` list, matching how `ReportRateFeature` is imported.

In `crates/openlogi-hidpp/Cargo.toml`, add `ghub-hidpp-gaming = { workspace = true }` and declare it in the workspace `[workspace.dependencies]` as `ghub-hidpp-gaming = { path = "crates/ghub-hidpp-gaming", version = "0.8.1" }`.

- [ ] **Step 3: Compile**

Run: `cargo check -p openlogi-hidpp`
Expected: clean. A `creatable` macro error usually means the derive wants a field named exactly `endpoint`.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p openlogi-hidpp`
Expected: pass, including `macro_registers_one_version_per_listed_impl` in `registry.rs`, which counts registered implementations and will notice the new row.

- [ ] **Step 5: Lint**

Run: `cargo clippy -p openlogi-hidpp --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/openlogi-hidpp crates/ghub-hidpp-gaming Cargo.toml
git commit -m "feat(hidpp): implement HID++ 0x8100 onboard profiles

The registry has listed 0x8100 by name with no implementation behind it. This
fills it in: memory layout, device mode, and the active profile.

Written against the reference G703, which reports the feature at version 0 and
sits in on-board mode with profile 1 active."
```

---

## Task 4: Read the real device

**Files:**
- Modify: `crates/openlogi-cli/src/diag.rs`

**Interfaces:**
- Consumes: `hidpp::feature::onboard_profiles::OnboardProfilesFeature` from Task 3.
- Produces: `openlogi diag onboard`, a read-only command.

This is the task that proves the previous three. It reads the actual G703 over the actual Lightspeed receiver and prints what the device says. Read the neighbouring `diag dpi` implementation first and follow its shape — how it resolves the active device, how it formats output, how it reports a device that does not support the feature.

**This command writes nothing.** Onboard memory holds the user's saved configuration, and a bad write bricks it into a state only G HUB on Windows can fix. Writes come in a later plan, behind an explicit flag, after a read path has been proven.

- [ ] **Step 1: Add the subcommand**

Add an `Onboard` variant to the `diag` subcommand enum with the help text `Read HID++ 0x8100 onboard-profile state from the active device`, and a handler that:

1. Resolves the active device the same way `diag dpi` does.
2. Returns a clear "device does not expose 0x8100" message when the feature is absent, rather than an error trace. That is the expected outcome for every non-gaming Logitech mouse.
3. Prints, one per line: memory model id, profile format id, macro format id, profile count, profile count out of box, button count, sector count, sector size, device mode, and the active profile.
4. When `ghub_models::model_for_hidpp_id` knows the device, prints its display name and compares the model table's slot count against the device's reported `button_count`, flagging a mismatch. A disagreement means the table is wrong, and it is better to find that here than in the GUI.

- [ ] **Step 2: Build**

Run: `cargo build -p openlogi`
Expected: clean.

- [ ] **Step 3: Run it against the hardware**

Run: `./target/debug/openlogi diag onboard`
Expected, with the G703 connected either way:

- `button_count` is **6**, matching the model table.
- `profile_count` is **5**, matching `onboard_profile_count`.
- Device mode reads `Onboard`.
- No mismatch warning.

If `button_count` disagrees with the table, the device is right and the table is wrong — fix `catalog.rs` and rerun. Record the real output in the commit message; it is the evidence this plan produced something true.

- [ ] **Step 4: Confirm it degrades**

Run the same command with the G703 unplugged and the receiver still attached, and confirm the "not supported" path prints a sentence rather than a panic or a trace.

- [ ] **Step 5: Lint**

Run: `cargo clippy -p openlogi-cli --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/openlogi-cli
git commit -m "feat(cli): add diag onboard, the first read of a gaming mouse

Reads 0x8100 state from the active device and cross-checks the reported button
count against the model table, so a wrong table is caught at the CLI instead of
in the GUI.

Read-only on purpose. Onboard memory holds the user's saved configuration and a
bad write leaves it in a state only G HUB on Windows can repair; writes come
later, behind a flag, once reads are proven.

Verified against the reference G703 over the Lightspeed receiver."
```

---

## Task 5: Report buttons for gaming devices

**Files:**
- Modify: `crates/openlogi-device/src/` — the probe path that decides a device's capabilities
- Modify: `crates/openlogi-core/src/config/identity.rs` — the `capabilities` record
- Test: alongside whichever module the capability decision lives in

**Interfaces:**
- Consumes: `ghub_models::model_for_hidpp_id`, `ghub_models::model_for_usb_id` from Task 1; the `0x8100` feature from Task 3.
- Produces: a device probe that sets `capabilities.buttons = true` for a device that exposes `0x8100` and appears in the model table.

Today, `capabilities.buttons` is false for the reference G703, which is why its config in `~/.config/openlogi/config.toml` records `buttons = false` and the GUI draws no button screen for it. The capability is decided by looking for `0x1b04`, which a gaming mouse never has.

- [ ] **Step 1: Find the decision**

Run: `grep -rn "buttons" crates/openlogi-device/src crates/openlogi-core/src/config/identity.rs`

Locate where `capabilities.buttons` is set during probing. Read enough of the surrounding probe to understand what it already knows about the device at that point — specifically whether it has the feature list and the model id available there.

- [ ] **Step 2: Write the failing test**

Write a unit test for the capability decision as a pure function over "which features the device reported" plus "its model id", asserting three cases:

1. A device with `0x1b04` and no model-table entry reports buttons — the existing behaviour must not regress.
2. A device with `0x8100` whose model id is in the table reports buttons.
3. A device with `0x8100` whose model id is **not** in the table does **not** report buttons, because without a slot table there is nothing to show the user.

If the decision is not currently a pure function, extract one and have the probe call it. That extraction is the point: a capability rule buried inside async probing cannot be tested.

- [ ] **Step 3: Run and watch it fail**

Run: `cargo test -p openlogi-device`
Expected: FAIL on cases 2 and 3.

- [ ] **Step 4: Implement**

Extend the rule so `0x8100` plus a known model also grants the capability.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p openlogi-device`
Expected: all three pass.

- [ ] **Step 6: Verify against the hardware**

```bash
cp ~/.config/openlogi/config.toml /tmp/openlogi-config-backup.toml
systemctl --user restart openlogi-agent
./target/debug/openlogi list
grep -A 3 "capabilities" ~/.config/openlogi/config.toml
```

Expected: the G703's `capabilities.buttons` is now `true`. The backup exists because the agent rewrites this file, and losing the user's device identity records would be a real loss.

- [ ] **Step 7: Lint the affected packages**

Run: `cargo clippy -p openlogi-device -p openlogi-core --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/openlogi-device crates/openlogi-core
git commit -m "feat(device): report buttons for gaming mice through 0x8100

The capability was decided by looking for 0x1b04, so every G-series mouse
probed as having no buttons at all. A device that exposes 0x8100 and appears in
the model table now reports them too.

A device with 0x8100 but no table entry still reports none, deliberately:
without a slot table there is nothing to show, and claiming buttons we cannot
name would push the failure into the GUI.

Verified on the reference G703, which now records buttons = true."
```

---

## Definition of done

`openlogi diag onboard` prints the G703's real onboard state, its reported button count agrees with the model table, and `openlogi list` shows the device with buttons. No write has touched onboard memory.

## What comes after

Each is its own plan, and each produces working software on its own.

| Plan | Delivers |
|---|---|
| **2 — Button capture** | The agent sees G703 button presses. Resolves the one open technical question in the spec at §10: whether `0x8110` button spy beats an evdev `grab`, given the device's second keyboard interface. |
| **3 — Macro engine** | `ghub-macro`: sequences with press/release granularity, the three G HUB repeat modes, and the guarantee that no key is ever left held. |
| **4 — Per-game profiles** | `ghub-profiles`: named profiles, window matching, the G-Shift layer, and the level-triggered reconciliation from spec §6. Includes making the GNOME Shell extension push rather than poll. |
| **5 — The GUI** | Profile list, macro recorder, G-Shift toggle on the button screen. |

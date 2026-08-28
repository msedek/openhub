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
///
/// Ten fields, but eleven bytes: `sector_size` is the one that takes two. The
/// device answers in a sixteen-byte long report, whose last five bytes are
/// reserved.
const INFO_LEN: usize = 11;

/// Decodes a `getOnboardProfilesInfo` response.
///
/// Trailing bytes are ignored: responses arrive in a fixed-size report, so
/// padding after the last field is normal.
///
/// # Errors
///
/// [`ParseError::ShortPayload`] when fewer than eleven bytes arrived.
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
        various_info: payload[10],
    })
}

#[cfg(test)]
mod tests {
    use super::{DeviceMode, ParseError, parse_info};

    /// A payload shaped like what a G703 returns: five profiles, six buttons,
    /// sixteen sectors of 256 bytes.
    ///
    /// Eleven bytes, not ten: `sector_size` occupies two of them, so the two
    /// fields behind it sit at offsets 9 and 10.
    #[test]
    fn parses_a_full_info_payload() {
        let payload = [
            0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x01, 0x00, 0x04, 0x00,
        ];

        let info = parse_info(&payload).unwrap();

        assert_eq!(info.memory_model_id, 0x01);
        assert_eq!(info.profile_format_id, 0x03);
        assert_eq!(info.macro_format_id, 0x01);
        assert_eq!(info.profile_count, 5);
        assert_eq!(info.profile_count_oob, 1);
        assert_eq!(info.button_count, 6);
        assert_eq!(info.sector_count, 0x10);
        assert_eq!(info.sector_size, 0x0100);
        assert_eq!(info.mechanical_layout, 0x04);
        assert_eq!(info.various_info, 0x00);
    }

    /// The sector size is the only multi-byte field, and HID++ is big endian.
    /// Getting this backwards would read 256 as 1, which is the kind of bug
    /// that only shows up when something writes to onboard memory later.
    #[test]
    fn reads_sector_size_big_endian() {
        let payload = [
            0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x02, 0x00, 0x04, 0x00,
        ];

        assert_eq!(parse_info(&payload).unwrap().sector_size, 512);
    }

    /// A short payload means the device answered something this code does not
    /// understand. Truncating silently would invent a device with zero buttons.
    #[test]
    fn rejects_a_short_payload() {
        let payload = [0x01, 0x03, 0x01];

        assert!(matches!(
            parse_info(&payload),
            Err(ParseError::ShortPayload {
                expected: 11,
                got: 3
            })
        ));
    }

    /// Responses arrive in a fixed-size report, so trailing padding is normal
    /// and must not be treated as an error.
    #[test]
    fn ignores_trailing_padding() {
        let payload = [
            0x01, 0x03, 0x01, 0x05, 0x01, 0x06, 0x10, 0x01, 0x00, 0x04, 0, 0, 0, 0,
        ];

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

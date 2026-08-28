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

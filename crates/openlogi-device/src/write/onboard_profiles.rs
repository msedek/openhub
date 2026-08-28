//! Reads of HID++ `0x8100` OnboardProfiles.
//!
//! Gaming mice keep their button map, DPI presets, report rate and lighting in
//! a bank of profiles in their own memory; `0x8100` is the door to it. This
//! module opens that door and reads through it, nothing more.
//!
//! **Read-only on purpose.** Onboard memory holds the configuration the user
//! saved on the device itself, and a wrong write leaves it in a state only the
//! vendor's own Windows software can repair. `setOnboardMode` (function 1) and
//! `setCurrentProfile` (function 3) exist on the feature and are deliberately
//! not reachable from here until a write path has been designed on its own
//! terms.

use std::sync::Arc;

use hidpp::{
    device::Device,
    feature::{CreatableFeature, onboard_profiles::OnboardProfilesFeature},
    protocol::v20::{ErrorType, Hidpp20Error},
};

use crate::backend::HidBackend;
use crate::channel::route::DeviceRoute;

use super::{WriteError, open_feature, with_route};

// The decoded payload types are `hidpp`'s to define; re-exported here so a
// caller of this module names one path rather than two.
pub use hidpp::feature::onboard_profiles::{DeviceMode, OnboardProfilesInfo};

/// Everything `0x8100` will say about a device without being written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnboardState {
    /// The device's onboard memory layout, from `getOnboardProfilesInfo`.
    pub info: OnboardProfilesInfo,
    /// Whether the device runs its own profile or defers to the host.
    ///
    /// `None` means the device answered with a mode value this build does not
    /// know, which is reported rather than guessed at.
    pub mode: Option<DeviceMode>,
    /// The onboard profile currently active. One-based on the wire.
    pub current_profile: u8,
}

/// Classify a HID++ error from a `0x8100` read.
///
/// A device that announces the feature but rejects one of its read functions
/// will keep rejecting it, so that maps to the permanent
/// [`WriteError::FeatureUnsupported`] — the same shape a device without the
/// feature at all produces, which is what lets one caller-side branch cover
/// "this mouse has no onboard profiles". Everything else is forwarded as text.
///
/// The text form is deliberate: [`super::HidppOperation`] crosses the IPC wire,
/// where its variant order *is* the format, and a diagnostic read does not
/// justify widening that contract.
fn classify_onboard_error(error: &Hidpp20Error) -> WriteError {
    match error {
        Hidpp20Error::Feature(ErrorType::Unsupported | ErrorType::InvalidFunctionId)
        | Hidpp20Error::UnsupportedResponse => WriteError::FeatureUnsupported {
            feature_hex: OnboardProfilesFeature::ID,
        },
        other => WriteError::Hidpp(format!("{other:?}")),
    }
}

/// Read the `0x8100` state of the device `route` reaches.
///
/// # Errors
///
/// [`WriteError::FeatureUnsupported`] when the device does not expose
/// `0x8100`, which is the ordinary answer for every non-gaming Logitech
/// device. Transport and protocol failures surface as their own variants.
pub async fn get_onboard_state(
    backend: &dyn HidBackend,
    route: &DeviceRoute,
) -> Result<OnboardState, WriteError> {
    let index = route.device_index();
    with_route(backend, route, move |channel| async move {
        get_onboard_state_on_channel(&channel, index).await
    })
    .await
}

async fn get_onboard_state_on_channel(
    channel: &Arc<hidpp::channel::HidppChannel>,
    index: u8,
) -> Result<OnboardState, WriteError> {
    let mut device = Device::new(Arc::clone(channel), index)
        .await
        .map_err(|_| WriteError::DeviceUnreachable { index })?;
    let feature = open_feature::<OnboardProfilesFeature>(&mut device).await?;

    let info = feature
        .get_info()
        .await
        .map_err(|e| classify_onboard_error(&e))?;
    let mode = feature
        .get_device_mode()
        .await
        .map_err(|e| classify_onboard_error(&e))?;
    let current_profile = feature
        .get_current_profile()
        .await
        .map_err(|e| classify_onboard_error(&e))?;

    Ok(OnboardState {
        info,
        mode,
        current_profile,
    })
}

#[cfg(test)]
mod tests {
    use hidpp::{
        feature::{CreatableFeature, onboard_profiles::OnboardProfilesFeature},
        protocol::v20::{ErrorType, Hidpp20Error},
    };

    use super::{WriteError, classify_onboard_error};

    fn is_unsupported_0x8100(error: &WriteError) -> bool {
        matches!(
            error,
            WriteError::FeatureUnsupported { feature_hex } if *feature_hex == OnboardProfilesFeature::ID
        )
    }

    #[test]
    fn a_rejected_function_reads_as_an_unsupported_feature() {
        for kind in [ErrorType::Unsupported, ErrorType::InvalidFunctionId] {
            let error = classify_onboard_error(&Hidpp20Error::Feature(kind));
            assert!(
                is_unsupported_0x8100(&error),
                "expected 0x8100 unsupported for {kind:?}, got {error:?}"
            );
        }
    }

    #[test]
    fn an_undecodable_payload_reads_as_an_unsupported_feature() {
        let error = classify_onboard_error(&Hidpp20Error::UnsupportedResponse);
        assert!(
            is_unsupported_0x8100(&error),
            "expected 0x8100 unsupported, got {error:?}"
        );
    }

    #[test]
    fn another_feature_error_stays_a_protocol_error() {
        // A busy device is transient: reporting it as "no onboard profiles"
        // would tell the user their gaming mouse is not a gaming mouse.
        let error = classify_onboard_error(&Hidpp20Error::Feature(ErrorType::Busy));

        assert!(
            matches!(error, WriteError::Hidpp(ref text) if text.contains("Busy")),
            "expected a protocol error naming Busy, got {error:?}"
        );
    }
}

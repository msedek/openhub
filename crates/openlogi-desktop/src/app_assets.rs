//! The settings app's GPUI [`AssetSource`].
//!
//! Composes the ring glyphs every frontend shares ([`openlogi_ui::action_icons`])
//! with the two sources only this app draws from: its embedded logo, and
//! gpui-component's bundled lucide set behind `IconName`. The overlay links
//! neither, which is why they live here and not in the shared crate.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use openlogi_ui::action_icons::ActionIcons;

/// Asset path [`AppAssets`] resolves to the embedded app logo.
pub const LOGO: &str = "openlogi.png";

/// The 1024×1024 app icon, embedded into the binary.
const LOGO_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../design/icon/openlogi.png"
));

include!(concat!(env!("OUT_DIR"), "/builtin_device_assets.rs"));

/// Generated local device geometry documents embedded in the application.
pub(crate) fn device_geometry_jsons() -> &'static [&'static str] {
    DEVICE_GEOMETRY_JSON
}

/// GPUI asset source: the app logo, the shared ring glyphs, then
/// gpui-component's bundled icons for everything else.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_BYTES)));
        }
        if let Some((_, bytes)) = DEVICE_ASSET_BYTES
            .iter()
            .find(|(resource_path, _)| *resource_path == path)
        {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        if let Some(bytes) = ActionIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_component_assets::Assets.list(path)
    }
}

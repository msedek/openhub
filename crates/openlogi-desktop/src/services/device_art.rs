//! The embedded device-art registry: which drawing a connected device gets,
//! and where its buttons and lit zones sit inside it.
//!
//! The drawings are the vendored Piper SVGs (`design/devices/svg`). `build.rs`
//! crops each one to its `Device` layer, embeds the result, and measures every
//! `buttonN` / `ledN` element into the table included below, so nothing here
//! parses SVG or touches the filesystem at runtime — a device resolves to a
//! `&'static DeviceArt` and the renderer hands the bytes straight to GPUI,
//! which rasterises SVG natively at whatever size the panel gives it.
//!
//! `svg-lookup.ini` keys drawings by USB vendor/product id. A HID++ device
//! reports one product id per transport it supports (wired USB, eQuad through
//! a receiver, Bluetooth), which is why [`art_for_products`] takes several and
//! matches on any of them: a G703 Hero plugged in is `c090`, and the same mouse
//! on its Lightspeed receiver is `4086`.

/// USB vendor id every HID++ device this app speaks to reports.
const LOGITECH: u16 = 0x046d;

/// Canvas size of a cropped drawing, in SVG user units. Every anchor below is
/// expressed in the same coordinates, so scaling both by the rendered size
/// keeps hotspots on their buttons at any panel size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtSize {
    /// Canvas width.
    pub width: f32,
    /// Canvas height.
    pub height: f32,
}

/// One identified element of a drawing — a physical button or a lit zone —
/// with the rectangle it occupies on the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtAnchor {
    /// The `N` in `buttonN` / `ledN`: the device's own numbering for the
    /// control, in the order its driver enumerates it.
    pub index: u32,
    /// Left edge on the canvas.
    pub x: f32,
    /// Top edge on the canvas.
    pub y: f32,
    /// Drawn width.
    pub width: f32,
    /// Drawn height.
    pub height: f32,
}

impl ArtAnchor {
    /// The anchor's centre — where a leader line points.
    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// One device drawing: the embedded SVG plus the geometry measured from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceArt {
    /// GPUI asset path the drawing's bytes are served on.
    pub resource: &'static str,
    /// The cropped canvas every anchor is measured against.
    pub canvas: ArtSize,
    /// Physical buttons, in device numbering order.
    pub buttons: &'static [ArtAnchor],
    /// Independently tintable lit zones, in device numbering order. The
    /// lighting UI colours these; they are kept addressable here so it never
    /// has to guess where a device's logo or DPI strip is drawn.
    pub leds: &'static [ArtAnchor],
}

/// One `svg-lookup.ini` section: a product name and the ids that select it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceArtMatch {
    /// The product name Piper lists the device under.
    pub name: &'static str,
    /// `(vendor, product)` pairs, from the section's `DeviceMatch` line.
    pub usb_ids: &'static [(u16, u16)],
    /// Index into [`DEVICE_ART`].
    pub art: usize,
}

include!(concat!(env!("OUT_DIR"), "/builtin_device_art.rs"));

/// The drawing for a device that reports these USB product ids, with the name
/// the registry lists it under.
///
/// Every reported id is tried because a HID++ device names itself differently
/// per transport and only one of those ids is the one the registry recorded.
#[must_use]
pub fn art_for_products(
    products: impl IntoIterator<Item = u16>,
) -> Option<(&'static DeviceArtMatch, &'static DeviceArt)> {
    let products: Vec<u16> = products.into_iter().filter(|id| *id != 0).collect();
    DEVICE_ART_MATCHES
        .iter()
        .find(|entry| {
            entry
                .usb_ids
                .iter()
                .any(|(vendor, product)| *vendor == LOGITECH && products.contains(product))
        })
        .map(|entry| (entry, &DEVICE_ART[entry.art]))
}

/// The generic mouse outline, for a device with no registry entry.
#[must_use]
pub fn fallback_art() -> &'static DeviceArt {
    &DEVICE_ART[FALLBACK_ART]
}

/// The embedded bytes of a drawing, for the GPUI asset source.
#[must_use]
pub fn art_bytes(resource: &str) -> Option<&'static [u8]> {
    DEVICE_ART_BYTES
        .iter()
        .find(|(path, _)| *path == resource)
        .map(|(_, bytes)| *bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The G703 Hero this project develops against, by its Lightspeed product
    /// id — the one the mouse reports through its receiver.
    #[test]
    fn the_g703_resolves_from_either_of_its_product_ids() {
        for product in [0xc090_u16, 0x4086] {
            let (entry, art) =
                art_for_products([product]).expect("the G703 Hero is in the registry");
            assert_eq!(entry.name, "Logitech G703 Hero");
            assert_eq!(art.resource, "device-art/logitech-g703.svg");
            assert_eq!(art.buttons.len(), 6, "the G703 drawing has six buttons");
            assert_eq!(art.leds.len(), 2, "the G703 has a logo and a DPI strip");
        }
    }

    /// A device the registry never heard of gets the generic outline, not an
    /// empty panel.
    #[test]
    fn an_unknown_product_id_has_no_entry_but_a_fallback() {
        assert!(art_for_products([0xffff_u16]).is_none());
        assert!(!fallback_art().buttons.is_empty());
        assert!(art_bytes(fallback_art().resource).is_some());
    }

    /// Every section of `svg-lookup.ini` must reach a drawing whose bytes are
    /// actually embedded — a typo in the ini is a build-time error, and this
    /// is the runtime half of that guarantee.
    #[test]
    fn every_registry_entry_resolves_to_embedded_art() {
        assert!(
            DEVICE_ART_MATCHES.len() >= 70,
            "the vendored registry covers 76 devices; found {}",
            DEVICE_ART_MATCHES.len()
        );
        for entry in DEVICE_ART_MATCHES {
            let art = DEVICE_ART
                .get(entry.art)
                .unwrap_or_else(|| panic!("{} points outside DEVICE_ART", entry.name));
            assert!(
                art_bytes(art.resource).is_some_and(|bytes| !bytes.is_empty()),
                "{} resolves to {}, which is not embedded",
                entry.name,
                art.resource
            );
            assert!(!entry.usb_ids.is_empty(), "{} matches nothing", entry.name);
        }
    }

    /// Anchors are only useful if they land inside the canvas they are
    /// measured against.
    #[test]
    fn every_anchor_sits_inside_its_canvas() {
        for art in DEVICE_ART {
            assert!(art.canvas.width > 0.0 && art.canvas.height > 0.0);
            for anchor in art.buttons.iter().chain(art.leds) {
                let (x, y) = anchor.center();
                assert!(
                    x >= 0.0 && x <= art.canvas.width && y >= 0.0 && y <= art.canvas.height,
                    "{} anchor {} centres at ({x}, {y}), outside {:?}",
                    art.resource,
                    anchor.index,
                    art.canvas
                );
            }
        }
    }
}

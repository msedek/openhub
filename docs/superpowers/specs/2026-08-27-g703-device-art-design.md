# G703 Device Art and Geometry Design

> Design approved on 2026-08-27. The G703 LIGHTSPEED HERO is the only device
> covered by this specification and the only device verified against physical
> hardware.

## Goal

Replace the inherited product render for the Logitech G703 LIGHTSPEED HERO
with original OpenHub artwork, exact button marker data, named lighting zones,
and an offline asset path consumed directly by the GPUI application.

The artwork must remain recognizable at the interactive model height of 320 px
and legible in the 120 px gallery treatment. Unsupported devices use the
existing synthetic silhouette until an original, hardware-verified definition
is added.

## Canonical definition

`tools/brand/generate_device_assets.py` is the source of truth. It contains a
declarative `DeviceSpec` made from numeric geometry: canvas dimensions, cubic
paths, polygons, ellipses, rounded rectangles, strokes, slots, and lighting
zones. Rendering code is generic and does not branch on the G703 model.

The generator writes both representations from the same data:

- `design/devices/g703_hero/device.svg` is the human-inspectable vector output.
- PNG derivatives are drawn directly with Pillow at a supersampled resolution
  and reduced with `Image.Resampling.LANCZOS`.

The generator never parses SVG. Editing generated SVG by hand is unsupported;
changes belong in the parametric Python definition and are regenerated.

## Visual construction

The illustration uses a `320 x 560` view box. The physical footprint follows
the measured `124 x 68 mm` length-to-width ratio, with transparent side margins
inside the canvas.

The body is dark graphite with restrained mineral edge lines. Its broad,
rounded tail, tapered front, and fuller right side communicate the G703's
right-handed ergonomic shape. Separate top panels describe the two primary
buttons. A center seam holds the scroll wheel, with the compact G6 button
directly behind it. G5 and G4 appear in front-to-rear order on the left flank.

OpenHub blue and cyan are reserved for the two real lighting zones:

- `logo`: the mark on the palm rest;
- `dpi_indicator`: the indicator strip on the left flank.

No decorative glow is placed elsewhere. Each zone is an independently named
shape and produces a transparent alpha mask for future live RGB tinting.

## Geometry schema

`design/devices/g703_hero/geometry.json` is generated beside the SVG. It uses
schema version 1 and the SVG view-box coordinate system:

```json
{
  "schema_version": 1,
  "device": {
    "id": "g703_hero",
    "name": "Logitech G703 LIGHTSPEED HERO",
    "canvas": { "width": 320, "height": 560 },
    "physical_dimensions_mm": { "length": 124, "width": 68, "height": 43 },
    "identifiers": {
      "hidpp_model_ids": ["4086"],
      "usb_product_ids": ["c090"]
    }
  },
  "slots": [
    {
      "id": "g1",
      "control": "left_click",
      "physical_location": "Left click",
      "evdev": { "name": "BTN_LEFT", "code": 272 },
      "marker": { "x": 112, "y": 112 },
      "label_side": "left"
    }
  ],
  "lighting_zones": [
    {
      "id": "logo",
      "svg_element": "lighting-logo",
      "masks": {
        "120": "lighting-logo-120.png",
        "320": "lighting-logo-320.png"
      }
    }
  ]
}
```

All six slots are present in physical G-number order and carry the measured
evdev name and numeric code. `control` bridges the new hardware slot to the
current `ButtonId` model without reviving inherited Logitech slot names.
`label_side` is authoritative at widths where the GUI has gutters on both
sides; the existing narrow-layout rule may still collapse all cards to the
left.

## Generated outputs

For every device definition, the generator emits:

- `device.svg`;
- `geometry.json`;
- `device-120.png` and `device-320.png`;
- one `lighting-<zone>-<height>.png` alpha mask per named zone and raster
  height.

Output filenames and ordering are deterministic. Raster width is derived from
the `320:560` canvas aspect ratio, giving `69 x 120` and `183 x 320` images.
PNG files use RGBA transparency.

## GUI integration

The generated 120 px and 320 px PNGs are embedded by `AppAssets`; installed
applications therefore require neither an adjacent data directory nor a
network request. The local catalog matches HID++ model id `0x4086` and returns
the thumbnail, interactive image, and parsed geometry together.

The runtime resolver does not load the inherited cache for normal application
construction, and automatic remote synchronization is disabled. A device with
no local definition resolves to `None`, preserving the synthetic silhouette.

`geometry.rs` gains a local-geometry path that scales absolute canvas markers
to the rendered image and honors explicit label sides. Its existing legacy
layout helpers remain available to their tests so navigation-pair grouping and
narrow-layout behavior keep equivalent coverage.

## Adding another device

Adding a device means adding another `DeviceSpec` data record and regenerating.
No rendering branch, SVG parser, Rust mapping table, or asset-source method is
added per device. The specification must provide:

- verified physical dimensions and top-view geometry;
- stable hardware identifiers;
- every physical slot, logical control, evdev name/code, marker point, and
  label side;
- each real lighting zone and its geometry;
- the source and date of hardware verification.

Devices without physical verification remain explicitly pending and must not
be inferred from inherited or cached product artwork.

## Verification

Python behavior tests generate into temporary directories and verify exact
outputs, JSON semantics, SVG structure, raster dimensions, visible alpha,
lighting-mask separation, and byte-for-byte determinism. Rust tests verify
local model resolution, marker scaling, explicit label sides, and preserved
label grouping. Final checks run the generator, the Python tests, formatting,
and `cargo test -p openlogi-desktop` when the host has the required system
libraries.

No commit is created.

# OpenHub Device Art

This directory contains original, offline product illustrations and their
interaction geometry. It must not contain artwork copied, traced, extracted,
or derived from Logitech software, OpenLogi's asset service, or any local
OpenLogi cache.

## Verification status

The G703 LIGHTSPEED HERO is the only device currently verified against real
hardware. Its physical layout, six evdev button codes, identifiers, and
dimensions were measured on 2026-08-27.

Every other device is pending. A model must not be presented as
hardware-verified merely because an inherited registry contains its name or a
product image exists elsewhere.

## Source of truth

`tools/brand/generate_device_assets.py` owns the canonical `DeviceSpec` data.
Each spec is a collection of numeric paths and primitives, slots, and lighting
zones. Generic emitters turn that data into SVG, JSON, and Pillow rasters.

Generated `device.svg`, `geometry.json`, and PNG files are committed outputs;
do not edit them by hand. The generator never reads or rasterizes SVG.

Run the generator from the repository root:

```sh
python3 tools/brand/generate_device_assets.py
```

Run its behavior tests with:

```sh
python3 -m unittest tools.brand.test_generate_device_assets -v
```

## Adding a device

Adding original art requires one new `DeviceSpec` data record. Do not add a
model-specific renderer or asset-source branch. Supply all of the following:

1. A stable snake-case device id and product name.
2. The device family and verified HID++/USB identifiers.
3. Physical length, width, and height in millimetres.
4. A top-view canvas and numeric shape primitives describing the body,
   controls, seams, and restrained technical details.
5. Every physical slot in hardware order, with:
   - stable slot id;
   - current logical-control bridge;
   - physical location;
   - evdev symbolic name and numeric code;
   - marker point in canvas coordinates;
   - explicit `left` or `right` label side.
6. Every real, independently controllable lighting zone. Each zone needs a
   stable id and its own shape list; decorative glow is not a lighting zone.
7. The hardware and date used to verify the definition.

The marker coordinates and shapes use the same absolute view box. There is no
percentage-based silhouette `origin` and no transparent-padding correction.

## Generated contract

For each `design/devices/<device-id>/` definition, generation writes:

- `device.svg` — human-inspectable vector output;
- `geometry.json` — versioned device, slot, marker, label, evdev, and lighting
  data;
- `device-120.png` — gallery raster;
- `device-320.png` — interactive-model raster;
- `lighting-<zone>-120.png` and `lighting-<zone>-320.png` — transparent alpha
  masks for future live RGB tinting.

The desktop build discovers every generated device directory and embeds its
JSON and PNG files in one local catalog. An unknown model receives the generic
synthetic silhouette and causes no device-art network request.

## Review checklist

- The shape follows measured proportions and is recognizable at 320 px.
- Buttons, seams, and real lighting zones remain legible at 120 px.
- Every marker lands on its physical control.
- Front/rear side-button order was verified on hardware.
- Label sides minimize leader-line crossings.
- Lighting masks overlap only their named physical zones.
- Two generator runs produce byte-identical outputs.

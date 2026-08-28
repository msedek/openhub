# G703 Device Assets Implementation Plan

> **For agentic workers:** Execute inline in the current session. The user
> explicitly requested work on `master`, no subagents, no commits, and final
> execution of the generator, Python tests, and the desktop crate tests.

**Goal:** Add original, parameter-generated G703 artwork and make the GPUI use
its embedded local geometry and rasters without a runtime CDN dependency.

**Architecture:** One declarative Python `DeviceSpec` owns every vector shape,
slot, and lighting zone. Generic Pillow/SVG emitters write the design outputs;
the desktop embeds those outputs and resolves model `0x4086` through a local
descriptor while unknown models retain the synthetic silhouette.

**Tech Stack:** Python 3, Pillow 12.1.1, standard-library `dataclasses`, JSON and
XML, Rust 1.98, serde, GPUI embedded assets, `unittest`.

## Global Constraints

- Original geometry only; do not inspect or derive from cached OpenLogi or
  Logitech artwork.
- English-only code, comments, generated metadata, and documentation.
- `tools/brand/generate_device_assets.py` is the sole canonical definition.
- The generator never parses SVG and depends only on Pillow plus the Python
  standard library.
- Raster heights are exactly 120 and 320 px and use supersampling followed by
  LANCZOS reduction.
- Only `logo` and `dpi_indicator` are lighting zones.
- The six measured evdev mappings are ground truth.
- Unsupported devices use the synthetic silhouette.
- Preserve equivalent coverage for every existing `geometry.rs` behavior test.
- Do not commit.

---

### Task 1: Parameterized device generator

**Files:**
- Create: `tools/brand/test_generate_device_assets.py`
- Create: `tools/brand/generate_device_assets.py`
- Generate: `design/devices/g703_hero/device.svg`
- Generate: `design/devices/g703_hero/geometry.json`
- Generate: `design/devices/g703_hero/*.png`

**Interfaces:**
- Produces: `generate_device_assets(root: Path) -> list[Path]`.
- Produces: `validate_device_assets(root: Path) -> None`.
- Consumes: immutable `DEVICE_SPECS: tuple[DeviceSpec, ...]` data.

- [ ] **Step 1: Write a failing filesystem-contract test**

```python
EXPECTED_OUTPUTS = (
    "design/devices/g703_hero/device-120.png",
    "design/devices/g703_hero/device-320.png",
    "design/devices/g703_hero/device.svg",
    "design/devices/g703_hero/geometry.json",
    "design/devices/g703_hero/lighting-dpi_indicator-120.png",
    "design/devices/g703_hero/lighting-dpi_indicator-320.png",
    "design/devices/g703_hero/lighting-logo-120.png",
    "design/devices/g703_hero/lighting-logo-320.png",
)

def test_generate_device_assets_writes_complete_contract(self) -> None:
    generator = load_generator()
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        written = generator.generate_device_assets(root)
        actual = tuple(sorted(path.relative_to(root).as_posix() for path in written))
        self.assertEqual(EXPECTED_OUTPUTS, actual)
```

- [ ] **Step 2: Run the test and confirm the missing-module failure**

Run: `python3 -m unittest tools.brand.test_generate_device_assets -v`

Expected: failure naming the missing `tools.brand.generate_device_assets`
module.

- [ ] **Step 3: Add semantic and raster tests before implementation**

Parse `geometry.json` and assert the literal slot tuples below, parse the SVG
root and named lighting groups, inspect the six PNG sizes/modes, and compare
SHA-256 digests from two temporary output roots:

```python
EXPECTED_SLOTS = (
    ("g1", "left_click", "BTN_LEFT", 272, "left"),
    ("g2", "right_click", "BTN_RIGHT", 273, "right"),
    ("g3", "middle_click", "BTN_MIDDLE", 274, "right"),
    ("g4", "back", "BTN_SIDE", 275, "left"),
    ("g5", "forward", "BTN_EXTRA", 276, "left"),
    ("g6", "dpi_toggle", "BTN_TASK", 279, "right"),
)
```

- [ ] **Step 4: Implement generic parametric emitters**

Define frozen data classes for points, cubic paths, ellipses, rounded
rectangles, polygons, lines, slots, zones, and devices. Sample cubic curves
from their numeric control points for Pillow, while writing the same controls
as SVG `C` commands. Render at 4x target scale, then downsample once.

- [ ] **Step 5: Define the G703 entirely as data**

Use a `320 x 560` canvas, the measured `124 x 68 x 43 mm` dimensions, a dark
graphite body, separate primary panels, center seam, wheel, G6 button, left
flank G5/G4 controls, and the two named blue/cyan lighting shapes. Do not add a
G703-specific branch to any renderer.

- [ ] **Step 6: Run the focused tests to green**

Run: `python3 -m unittest tools.brand.test_generate_device_assets -v`

Expected: all generator tests pass with no warnings.

---

### Task 2: Embedded local catalog

**Files:**
- Modify: `crates/openlogi-desktop/src/app_assets.rs`
- Modify: `crates/openlogi-desktop/src/services/assets.rs`
- Modify: `crates/openlogi-desktop/src/services/assets/sync.rs`
- Modify: `crates/openlogi-desktop/src/app/home.rs`
- Modify: `crates/openlogi-desktop/src/features/lighting/visual.rs`
- Modify: `crates/openlogi-desktop/src/features/mouse/view.rs`

**Interfaces:**
- Produces: `ResolvedAsset::image_source() -> gpui::ImageSource`.
- Produces: `ResolvedAsset::hero_image_source() -> Option<gpui::ImageSource>`.
- Produces: parsed `DeviceGeometry` on the G703 asset.

- [ ] **Step 1: Write a failing resolver test**

Construct `DeviceModelInfo` with model id `0x4086`, resolve it through
`AssetResolver::new()`, and assert depot `g703_hero`, the two embedded resource
names, canvas `320 x 560`, and six slots. Also assert an unrelated model
resolves to `None` from a normal resolver.

- [ ] **Step 2: Run only the new resolver test and confirm RED**

Run: `cargo test -p openlogi-desktop g703_resolves_from_the_embedded_catalog`

Expected: failure because no local descriptor exists.

- [ ] **Step 3: Embed generated bytes in `AppAssets`**

Register stable resource names under
`device-assets/g703_hero/{device-120.png,device-320.png}` with `include_bytes!`
from `design/devices/g703_hero/`.

- [ ] **Step 4: Add local schema types and model resolution**

Deserialize schema version, canvas, slots, marker points, label sides, evdev
metadata, and lighting-zone masks from the generated JSON. Resolve `0x4086`
before any legacy helper and carry embedded image identifiers in
`ResolvedAsset` without requiring filesystem paths.

- [ ] **Step 5: Make ordinary resolution local-only**

`AssetResolver::new()` must not load the user cache or bundled inherited
index. `sync::should_run` returns false and `load_registry` returns a local-only
error before any HTTP operation. Keep legacy resolver constructors and helpers
available to their isolated unit tests until the broader inherited subsystem
is removed separately.

- [ ] **Step 6: Update the three image consumers**

The gallery uses `hero_image_source()`. The mouse and lighting panels use
`image_source()`. Filesystem-backed assets continue to convert from their
existing `PathBuf` values.

- [ ] **Step 7: Run focused resolver tests to green**

Run: `cargo test -p openlogi-desktop g703_resolves_from_the_embedded_catalog`

Expected: pass.

---

### Task 3: Exact marker and label geometry

**Files:**
- Modify: `crates/openlogi-desktop/src/features/mouse/geometry.rs`
- Modify: `crates/openlogi-desktop/src/features/mouse/view.rs`

**Interfaces:**
- Extends: `asset_hotspots_for_png` with absolute local-canvas markers.
- Produces: `asset_labels_from_hotspots(asset, hotspots, mouse_h, distribution)`.

- [ ] **Step 1: Add failing absolute-coordinate and side tests**

Create a local test asset with a `320 x 560` canvas. Assert that the G1 marker
at `(112, 112)` scales to `(56, 56)` on a `160 x 280` render, before hotspot
half-width adjustment. Assert G1/G4/G5 select `Side::Left` and G2/G3/G6 select
`Side::Right` in `BothSides` mode.

- [ ] **Step 2: Run focused geometry tests and confirm RED**

Run: `cargo test -p openlogi-desktop features::mouse::geometry::tests`

Expected: the new tests fail because local canvas coordinates and declared
sides are not consumed.

- [ ] **Step 3: Implement local marker scaling**

For local geometry, compute `x = marker.x / canvas.width * mouse_w` and
`y = marker.y / canvas.height * mouse_h`. Keep the inherited percentage path
for isolated compatibility tests.

- [ ] **Step 4: Refactor label placement without losing grouping behavior**

Extract the current vertical spacing and Back/Forward adjacency logic into one
helper that accepts initial sides. `labels_from_hotspots` supplies its current
automatic sides; `asset_labels_from_hotspots` supplies declared local sides or
forces `Left` in narrow mode.

- [ ] **Step 5: Run all geometry tests to green**

Run: `cargo test -p openlogi-desktop features::mouse::geometry::tests`

Expected: all old and new geometry tests pass.

---

### Task 4: Contributor documentation and final verification

**Files:**
- Create: `design/devices/README.md`
- Regenerate: `design/devices/g703_hero/*`

- [ ] **Step 1: Document the device-definition contract**

Explain the Python source-of-truth rule, required measurements and identifiers,
slot and lighting-zone data, generator command, output contract, validation
commands, and the ban on inherited/cached artwork. State that the G703 is the
only hardware-verified device and every other model is pending.

- [ ] **Step 2: Generate production outputs**

Run: `python3 tools/brand/generate_device_assets.py`

Expected: eight generated files listed and validated.

- [ ] **Step 3: Run Python tests**

Run: `python3 -m unittest tools.brand.test_generate_device_assets -v`

Expected: all tests pass.

- [ ] **Step 4: Run formatting and Rust tests**

Run: `cargo fmt --all -- --check`

Run: `cargo test -p openlogi-desktop`

Expected: both exit zero. If desktop compilation stops because a host system
library is absent, preserve the exact error and report that test as not run;
do not install packages or bypass the dependency.

- [ ] **Step 5: Inspect both production raster sizes**

Open `device-120.png`, `device-320.png`, and the four lighting masks. Confirm
transparent margins, recognizable G703 proportions, separate controls, and no
light outside the two real zones.

- [ ] **Step 6: Review the final diff and deterministic regeneration**

Run the generator a second time, then run `git diff --check` and
`git status --short`. Confirm regeneration creates no additional diff and that
the pre-existing user modification to `NOTICE` remains untouched.

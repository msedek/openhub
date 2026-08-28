# OpenHub Brand Assets Implementation Plan

> **For agentic workers:** Execute inline in the current session. The user explicitly
> requested work on `master`, no subagents, and no commits.

**Goal:** Build one deterministic Pillow generator that writes the complete OpenHub
brand asset set to the repository's existing packaging paths.

**Architecture:** `generate_assets.py` owns a normalized geometry model and renders
the mark into masks. Color application, platform container encoding, SVG composition,
and output validation consume those masks without redefining the logo. A standard
library `unittest` module exercises generation in a temporary root.

**Tech Stack:** Python 3, Pillow 12.1.1, `unittest`, XML text generation.

## Global Constraints

- Original artwork only; no OpenLogi brand geometry or visual references.
- Exact output paths and dimensions from the user request are immutable.
- Pillow is the only non-stdlib dependency.
- Small rasters derive from a 1024 master using `Image.Resampling.LANCZOS`.
- Tray artwork is one-color RGBA on transparency and remains readable at 44 px.
- The script is deterministic and runs as `python3 tools/brand/generate_assets.py`.
- Do not commit.

---

### Task 1: Executable output contract

**Files:**
- Create: `tools/brand/test_generate_assets.py`
- Create: `tools/brand/generate_assets.py`

**Interfaces:**
- Consumes: a destination root as `pathlib.Path`.
- Produces: `generate_assets(root: Path) -> list[Path]` and
  `validate_assets(root: Path) -> None`.

- [ ] Write an integration test that imports `generate_assets`, renders into
  `TemporaryDirectory`, and compares the returned relative paths with the literal
  20-file contract.
- [ ] Run `python3 -m unittest tools.brand.test_generate_assets -v`; confirm it fails
  because the generator module does not exist.
- [ ] Add the normalized geometry model, mask renderers, directory creation, and all
  format writers needed to satisfy the contract.
- [ ] Re-run the focused test until it passes.

### Task 2: Platform properties and small-size behavior

**Files:**
- Modify: `tools/brand/test_generate_assets.py`
- Modify: `tools/brand/generate_assets.py`

**Interfaces:**
- `render_mark(size: int, variant: str, color: tuple[int, int, int, int]) -> Image`
  returns an RGBA mark on transparency.
- `render_app_icon(variant: str) -> Image` returns exactly `1024 × 1024` RGBA.

- [ ] Add tests with literal dimensions for every PNG, SVG root, ICO frame, and ICNS
  representation; test that tray non-transparent pixels have exactly one RGB value.
- [ ] Run the tests and confirm the new assertions fail on the minimal implementation.
- [ ] Add 1024-master LANCZOS resizing, ICO/ICNS encoders, transparent monochrome tray
  masks, and SVG metadata until the assertions pass.
- [ ] Mutate the expected tray color and one size locally to confirm the tests detect
  those regressions, then restore them.

### Task 3: Production generation and deterministic verification

**Files:**
- Modify: the exact icon, tray, DMG, ICNS, and banner paths listed in the specification.

**Interfaces:**
- CLI entry point resolves the repository root from the script location, calls
  `generate_assets`, then `validate_assets`, and prints a compact manifest.

- [ ] Run `python3 tools/brand/generate_assets.py` from the repository root.
- [ ] Run it again and compare SHA-256 values for every generated file.
- [ ] Run `python3 -m unittest tools.brand.test_generate_assets -v` fresh.
- [ ] Inspect the normal master, 16 px icon enlarged with nearest-neighbor, Prism,
  three tray icons, banner, and raster previews of both SVGs.
- [ ] Run a manifest check that prints path, format, dimensions, mode, and byte size;
  reject empty, wrong-sized, wrong-format, or unexpectedly tiny outputs.
- [ ] Review `git diff --stat`, `git status --short`, and preserve all unrelated user
  changes. Do not commit.

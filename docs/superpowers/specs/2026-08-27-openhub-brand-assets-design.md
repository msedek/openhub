# OpenHub — brand system design

## Objective

Create an original identity for OpenHub, independent from OpenLogi's reserved assets,
that works as an application icon, tray silhouette, installer background, and
banner. The entire system comes from a single geometric definition and is
regenerated with Pillow without randomness or external services.

## Visual philosophy: Orbital Circuit

Orbital Circuit expresses control through compact masses and visible relationships.
A stable core organizes six unequal nodes; the information lies in their scale,
rhythm, and linkage, not in ornament. The mark is recognized first as a hub and then,
by those familiar with the product, as the functional map of a mouse's six buttons.

The shape starts in monochrome. Thick bridges turn the core and nodes into a single
silhouette, with broad openings that remain clear at 16 px. The two main
buttons dominate the upper axis, the wheel takes the form of a vertical capsule, the two
side buttons form an offset pair, and the button behind the wheel acts as a counterweight.
The asymmetry is deliberate, and the whole is optically balanced upward.

The color avoids a decorative rainbow. Blue graphite, electric blue, and cool cyan
provide contained energy; a mineral white supports the contrast. Gradients
describe depth and flow around a shape that already works without them.
Each transition must look carefully calibrated at an editorial level, without noise,
plastic highlights, or effects that depend on a large scale.

The composition uses negative space as material. In the icon, a rounded tile
protects the constellation; in the banner and DMG backgrounds, fragments of the topology
expand into a quiet technical field. Text is sparse and is
reserved for the OpenHub name outside the icon.

The execution must feel meticulously constructed: clean curves, consistent optical
weights, and unambiguous hierarchy. The Prism variant preserves the family but opens
the core with a rounded triangular cutout and shifts the field to amber–cyan;
this makes it distinct both in color and as a 22 pt monochrome silhouette.

## Master geometry

- Normalized system: `1024 × 1024` canvas.
- Mark: circular core joined to six nodes by thick, rounded bridges.
- Hierarchy: two large primary nodes, a capsule wheel, two medium side
  nodes, and one small auxiliary node.
- Optical compensation: the mark sits slightly high within the tile, and the weight of
  the side nodes is balanced by a shorter extension on the opposite side.
- The Prism variant uses the same outer silhouette and adds a central opening that
  remains visible in the tray.

## Color system

| Function | Color |
|---|---|
| Deep night | `#08111F` |
| Blue graphite | `#12233A` |
| Electric blue | `#2563EB` |
| Cool cyan | `#22D3C5` |
| Mineral white | `#F2F7FA` |
| Prism amber | `#F59E42` |
| Prism coral | `#F05D5E` |

The standard icons use a night→graphite field with blue→cyan accents. Prism uses
a petrol→blue field and an amber→coral accent, along with the geometric opening.

## Deliverables and rules

- The canonical script is `tools/brand/generate_assets.py`.
- It depends only on Pillow and the standard library.
- Each small raster is derived from the 1024 master using LANCZOS.
- Tray icons are generated from a monochrome mask, not from the color icon.
- ICO and ICNS contain the scales required by their platforms.
- The DMG SVGs reuse the mark and palette, leaving clear the areas where
  Finder places the application and the link to Applications.
- The banner measures `1280 × 640`, contains the OpenHub name, and no other brand.
- No consumer path is modified, and no legacy file is renamed.
- No commit is created.

## Verification

The tests generate into a temporary directory, check the complete list of
outputs, dimensions, modes, ICO/ICNS frames, tray transparency,
monochrome rendering, and SVG structure. Final generation runs twice and compares
hashes to demonstrate determinism. The masters, 16 px icon, tray, Prism, banner,
and both DMG backgrounds are then visually inspected.

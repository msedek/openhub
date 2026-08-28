# Device illustrations

These 67 SVG files, the `svg-lookup.ini` that maps a device's USB ids to one of
them, and `SVG-FORMAT.md` describing how they are drawn, come from
[**Piper**](https://github.com/libratbag/piper), the GTK configuration tool for
libratbag.

    Copyright © 2016–2022 the libratbag team
    https://github.com/libratbag/piper
    Licensed under GPL-2.0-or-later

They are used here under the "or later" clause, as GPL-3.0-or-later. **This is
why OpenHub is GPL-3.0-or-later** and not the permissive MIT/Apache-2.0 it
inherited from OpenLogi. Using this artwork was a deliberate trade: copyleft in
exchange for accurate illustrations of 76 devices, drawn by people who own the
hardware.

Not modified. If OpenHub ever needs a change to one of these — a new device, a
correction — the change belongs upstream in Piper first, where every project
that renders these gets it.

## What they carry beyond the drawing

Each file follows the layout documented in `SVG-FORMAT.md`, which makes them
data as much as art:

| Element id | Meaning |
|---|---|
| `buttonN` | The Nth physical button, positioned on the device |
| `buttonN-leader` | Where that button's callout label belongs |
| `buttonN-path` | The line joining the two |
| `ledN` | The Nth lit zone |
| `ledN-leader`, `ledN-path` | The same, for lighting |

So the button anchor points arrive already placed. That is the part that would
otherwise have to be measured by hand for every device, and getting it wrong
puts the GUI's clickable hotspots somewhere the button is not.

## Coverage

76 device entries, 52 of them Logitech, including the `G703 Hero`
(`usb:046d:c090;usb:046d:4086`) this project develops against.

A device with no entry falls back to `fallback.svg` — which is not a generic
outline but Piper's placeholder joke: a cartoon mouse holding a "404" sign, with
`buttonN` elements on its paws. It is a legible "no artwork for this device"
signal rather than a wrong diagram, but it is not a neutral silhouette, and
anything that shows it should mean to.

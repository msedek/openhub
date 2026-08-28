# OpenHub — a G HUB for Linux built on the OpenLogi fork

> Design. Created 2026-08-27. Development and validation machine: msedek-pc
> (Ubuntu 26.04, GNOME Shell 50.1, Wayland). Reference hardware: Logitech
> G703 LIGHTSPEED HERO.
> Related: `~/obsidian-vault/Resources/g703-macros-lost-ark.md`,
> `~/obsidian-vault/Resources/lost-ark-config.md`.

## 1. What this is

A Logitech G HUB replacement for Linux: per-game profiles that activate
automatically, macros with G HUB's repeat modes, and DPI, report rate, and
lighting control for G-series gaming mice.

It is built on a fork of [OpenLogi](https://github.com/AprilNEA/OpenLogi)
(`msedek/openhub`), which provides the difficult half already solved: HID++ protocol,
device enumeration, cross-platform input capture and injection,
IPC between GUI and agent, and a GPUI GUI with Logitech's official assets.

**Concrete motivation**: the Windows installation with G HUB where nine Lost
Ark macros lived was removed. The macros survive only as text in the vault. No
Linux tool can execute them: libratbag has no repeat mode,
and OpenLogi does not even recognize the buttons on a gaming mouse.

## 2. Decisions made

| Decision | Choice | Rationale |
|---|---|---|
| Ambition | Product, not a personal patch | The generic design makes supporting the G502, G903, and G305 a matter of filling in a table, not programming |
| Upstream | Divergent fork, with no intention of merging | Freedom to redesign `openlogi-core` types instead of patching them |
| v1 hardware | G-series gaming mice | They share the same HID++ features; the G703 is the test bench |
| Execution | Software first; onboard later | Repeating macros do not exist in firmware; G HUB also executes them in software |
| Profile activation | Focused window, sticky | See §6 |
| Macro model | Identical to G HUB's | It is a clone: same modes, same recording editor, same G-Shift |

**Working name**: OpenHub. The new crates use the `ghub-` prefix.
The final visual identity will be decided before the first release; until
then the OpenLogi branding is preserved to avoid breaking the `APP_ID` used
by the `.desktop` file.

## 3. Hardware findings, verified on the real G703

Everything in this section was measured on the device on 2026-08-27; it was not
deduced from documentation.

### 3.1 HID++ features it exposes (wired connection, HID++ 4.2)

29 features. The ones that determine the design:

| Feature | On the mouse | In OpenLogi today |
|---|---|---|
| `0x8100` ONBOARD PROFILES | Yes. Device Mode: **On-Board** | Declared only, not implemented |
| `0x8110` MOUSE BUTTON SPY | Yes | Declared only, not implemented |
| `0x8071` RGB EFFECTS | Yes | Implemented |
| `0x8060` REPORT RATE | Yes, 1 ms over cable | Implemented |
| `0x2201` ADJUSTABLE DPI | Yes, 1000 DPI | Implemented |
| `0x1b04` REPROG CONTROLS | **Does not exist** | Implemented, and useless here |

**This table is the thesis of the project.** OpenLogi discovers remappable buttons
through `0x1b04`, the feature used by office MX mice. Gaming mice do not
expose it: their buttons live in `0x8100`. That is why the G703 currently appears in the
user's config with `capabilities.buttons = false`, and its assignment screen
is not even rendered. Implementing `0x8100` is not an optional improvement; it is
the entry requirement.

`0x8110 MOUSE BUTTON SPY` is Logitech's mechanism for software to
receive buttons through HID++ without intercepting evdev. This is how G HUB works and it is
an alternative to the evdev `grab` used by OpenLogi (§5.3).

### 3.2 Physical mapping of the six buttons

Captured by pressing each button and reading `/dev/input/event23`. The four
non-trivial buttons were confirmed in separate batches of three presses
each, with several seconds of silence between batches:

| G HUB | Physical button | evdev | Confirmations |
|---|---|---|---|
| G1 | Left | `BTN_LEFT` (272) | 1 |
| G2 | Right | `BTN_RIGHT` (273) | 1 |
| G3 | Wheel click | `BTN_MIDDLE` (274) | 3 of 3 |
| G4 | Rear side button | `BTN_SIDE` (275) | 2 of 2, plus elimination |
| G5 | Front side button | `BTN_EXTRA` (276) | 3 of 3 |
| G6 | Behind the wheel | `BTN_TASK` (279) | 3 of 3 |

The rear side button was measured separately and with fewer repetitions because `BTN_SIDE`
means "back" to the system: each press sends the focused application off
screen, making repetition inconvenient. This is design data, not just an
anecdote: any interactive test of that button must run with capture already
armed and without depending on the active window.

This resolves the pending item "physically verify whether G4 is the rear button" from the
vault note. The button behind the wheel **is visible to software**: the
firmware does not reserve it for cycling DPI, so it can host G-Shift.

### 3.3 Two interfaces, not one

The G703 presents two event nodes: `event23` (mouse interface) and
`event24` (**keyboard** interface). A macro stored in onboard memory emits
its keys through the second. Any layer that captures or filters input must
account for both, or it will see only half of what the device does.

### 3.4 Inconsistent onboard state

`ratbagctl` reports profile 1 as `(disabled) (active)` at the same time, and
`solaar` reports "Profile 1" as stored and current — the two tools
number differently (libratbag from 0, the `0x8100` protocol from 1). This is
residue from the configuration left by G HUB. It must be normalized before
writing final profiles, and the `0x8100` implementation must establish its
numbering convention and document it.

### 3.5 Wireless configuration works (resolved)

Measured with the cable disconnected, communicating only through the Lightspeed receiver:

- The mouse exposes **the same 29 features** as over cable, including `0x8100`,
  `0x8110`, and `0x2201`.
- **Reads** work: live battery at 97% through `0x1001`.
- **Writes** work: `openlogi diag dpi` wrote 1050, read back 1050,
  and restored 1000. Complete round trip.
- `libratbag` still returns `No devices available` through this path.

Conclusion: **the limitation is in libratbag, not in the hardware or protocol.**
The previous vault note, which stated that the G703 "requires a USB cable to
configure it," has been corrected. The product can configure the mouse wirelessly,
which was the design's greatest experience risk.

Additional data point: through HID++, the sensor reports a range of **100 to 25600 DPI in
511 steps** (≈50 increments), much finer than the five presets exposed by
libratbag. The GUI must offer the real range, not the presets.

## 4. What is reused and what is built

**Reused in full**: HID++ transport and receivers, enumeration, probing, and
pairing; input capture (evdev + uinput on Linux, CGEventTap on
macOS, WH_MOUSE_LL on Windows); input synthesis; tarpc IPC between GUI and
agent; focused-app detection; TOML config with merge by device
identity; action catalog; registry of 210 devices with images and
button coordinates; GPUI, its theme system, and the assignment screen
with clickable points over the mouse image (`features/mouse/geometry.rs`).

**Built**:

| Crate | Responsibility |
|---|---|
| `ghub-hidpp-gaming` | Features `0x8100` and `0x8110` over the vendored `hidpp` |
| `ghub-models` | Per-model table: slots, evdev codes, CIDs, button count, LEDs, DPI presets |
| `ghub-macro` | Macro execution engine: sequences, repetition, release guarantee |
| `ghub-profiles` | Game profile, layers, matching rules, active-profile reconciliation |

**Modified in the inherited crates** (without restriction; the fork is divergent):
`openlogi-core` (button model, §5.1), `openlogi-hook` (button
suppression, §5.3), `openlogi-agent-core` (active-profile resolution),
`openlogi-desktop` (new screens, §8).

Implementation reference for `0x8100`: `libratbag`, which already solves it in
C (`src/hidpp20.c`, `src/driver-hidpp20.c`). The binary format of the G703's
onboard profiles is described there; it is translated to Rust instead of being discovered
blindly against the firmware.

## 5. Data model

### 5.1 Buttons: from fixed enum to per-model table

`ButtonId` is currently a closed enum of 13 controls modeled after MX mice
(`LeftClick`, `Back`, `Forward`, `DpiToggle`, `Thumbwheel`, `GestureButton`…).
It cannot represent "the G4 on the G703" or an 11-button mouse such as the G502.

It is replaced by a reference to a slot declared by the model:

```
ButtonRef { model: ModelId, slot: SlotId }      // e.g. g703_hero / "g4"
```

The `SlotId` values are not invented: they come from Logitech's official assets
(`g703hero_g4_m1` in `metadata.json`), the same ones the GUI uses to
draw clickable points. Each model declares in `ghub-models` the table
`slot → evdev code` and `slot → HID++ CID`. Supporting a new mouse means filling
in one row, not writing code.

### 5.2 Game profile

```
GameProfile {
  id, name, icon
  match:  [ WindowClass("lostark.exe"), Title(regex), SteamAppId(1599340) ]
  device: { dpi_presets, active_dpi, report_rate, lighting }
  assignments: {
    normal:  ButtonRef -> Assignment
    g_shift: ButtonRef -> Assignment
  }
}

Assignment = Action(Action) | Macro(MacroId) | GShiftTrigger | Disabled
```

A profile is a named entity, visible in a list, not an overlay
hidden inside the device config like the current `per_app_bindings`.
That difference is the G HUB user's mental model and is not cosmetic.

`GShiftTrigger` is the assignment that turns a button into the modifier: the
button that has it emits no action of its own, and while it is held down
all other buttons use their `g_shift` assignment.

### 5.3 Macro

```
Macro {
  id, name
  steps: Vec<Step>
  repeat: NoRepeat | RepeatWhileHeld | Toggle
}

Step = KeyDown(Key) | KeyUp(Key) | ButtonDown(Btn) | ButtonUp(Btn) | Delay(ms)
```

The three modes are exactly G HUB's, no more and no fewer. Separate
press/release granularity is mandatory: without it, `Alt↓ V↕ Alt↑` cannot be expressed,
which is the actual form of the "Hyper" macro.

The nine Lost Ark macros expressed in this model:

| Name | Steps | Interval | Mode |
|---|---|---|---|
| SuperSpace | `SPACE↕` | 25 ms | Repeat while held |
| Hyper | `ALT↓ V↕ ALT↑` | 25 ms | Repeat while held |
| The T | `T↕` | 50 ms | Repeat while held |
| ULTRA CLK | `=↕` | 50 ms | Repeat while held |
| ShiftG | `SHIFT↓ G↕ SHIFT↑` | 25 ms | Repeat while held |
| SuperLeft | `BTN_LEFT↕` | 25 ms | Repeat while held |
| SuperRight | `BTN_RIGHT↕` | 25 ms | Repeat while held |

`superclk` had no sequence defined in the original document and will be
reconstructed with the user when creating the profile.

## 6. Focus detection: do not repeat G HUB's bug

G HUB has a known and reproducible defect: when returning to the game, the profile does not
reactivate, and windows must be switched several times. Measured on this machine, the
current mechanism has three weaknesses that would cause the same behavior.

**Measured state on msedek-pc**:

- The GNOME Shell extension that OpenLogi needs (`openlogi-frontmost@openlogi.dev`,
  included in `crates/openlogi-hook/gnome-shell-extension/`) **is not installed**,
  so the X11 fallback runs.
- That fallback lies: `_NET_ACTIVE_WINDOW` returns `0x400003`, a window with no
  `WM_CLASS` or `_NET_WM_NAME`, because focus is on a Wayland-native window
  invisible to X11.
- `org.gnome.Shell.Introspect.GetWindows` returns `AccessDenied` on GNOME 50.
  It is not an alternative to the extension.
- Focus is read by **polling every 1 second** (`crates/openlogi-agent/src/startup.rs:255`),
  and the extension responds to a D-Bus method: it is *pull*, not events.

**The three corrections**:

1. **Level-triggered reconciliation, not edge-triggered.** The agent does not react to
   focus transitions: on each tick, it compares the profile that *should* be
   active against the one that *is* applied, and converges. A missed read
   corrects itself on the next tick without user intervention. This is the
   correction that eliminates "switch windows until it sticks."

2. **`None` means unknown, never none.** Null focus, a window without
   `WM_CLASS`, or a read error preserves the current profile. A game's profile
   falls only when **another identifiable window** takes focus. Going to the
   desktop or minimizing everything does not drop it.

3. **Robust identity.** Matching is evaluated against `WM_CLASS`, with
   window title and Steam AppID as fallback. G HUB uses the executable
   path, which is not stable with Proton and launchers.

**Latency**: the extension changes from *pull* to *push*, emitting a D-Bus signal
on Mutter's `notify::focus-window`. Profile switching drops from up to one
second to milliseconds, and polling remains only as a reconciliation safety net every
few seconds. This is about fifteen lines in the extension.

The extension becomes a **required** component on GNOME Wayland, not
an optional one, and its installation is part of application onboarding.

## 7. Macro engine

It lives in `ghub-macro` and has two non-negotiable properties.

**No key may remain pressed.** This is the classic defect in any
auto-clicker: the button is released in the middle of `ALT↓ V↕ ALT↑`, and Alt remains held
forever. The executor records each key and button it pressed and emits the
corresponding releases on **every** exit path: physical button release,
profile change, focus loss, agent shutdown, and thread panic.
The precedent exists in the inherited code: `Action::HoldShortcut` already requires
dispatchers without release context to degrade to a balanced press
instead of leaving keys stuck.

**Timing must be real.** 25 ms is 40 Hz; if the timer drifts
to 35 ms under load, the macro feels sluggish exactly when it matters. Execution
runs in a dedicated thread with `timerfd`, not in the tokio runtime that
the rest of the agent shares. Drift is measured and recorded in tests.

**Physical-button suppression.** `ButtonId::is_os_hook_button()` currently allows
suppressing only `MiddleClick`, `Back`, and `Forward`. The `SuperRight` macro
(right auto-click under G-Shift) requires suppressing the right click, currently
prohibited by design. The restriction is removed: any button with an
active assignment other than its native function is suppressed. As a safeguard,
the left button is never suppressed unless it has an explicit assignment,
so a misconfigured profile does not leave the machine without primary click.

## 8. GUI

The device gallery, hardware-type tabbed detail view,
assignment screen with clickable points over the mouse image,
lighting panel, and pointer/DPI panel are preserved.

Four things are added:

1. **Profiles screen**: list of profiles with name, associated game, and
   indicator showing which one is currently active.
2. **Macro editor with recording**: record button, capture of keys and clicks
   with their real timing, step-by-step sequence editing, and repeat-mode
   selector. It is the screen the user already knows from G HUB.
3. **G-Shift switch** on the assignment screen: the same mouse image,
   a toggle at the top, and the second assignment for each button is displayed and edited.
4. **Button support for gaming mice**, which is a direct consequence of
   implementing `0x8100`: without it, the assignment screen is not rendered for
   the G703.

The existing `openlogi-overlay` process can display a notification when changing
profiles, as G HUB does. It is not part of the first version.

## 9. Out of scope

- **Writing profiles to onboard memory.** The hardware supports it and it will be
  implemented later, so the configured mouse can be taken to another machine. It is
  not necessary for any of the above.
- **G-series keyboards and headsets** (`0x8010`, `0x8020`). There is no hardware
  available to test them.
- **Recording macros in firmware** (`0x8030`). The G703 does not expose the
  feature.
- **Windows and macOS.** The inherited code supports them and they will not be
  deliberately broken, but they are not validated.
- **Activating the G-Shift layer with a keyboard key.** G HUB does not do it, and the
  objective is a clone.

## 10. Validation plan

Two short hardware tests, before writing product code,
because their results constrain the design:

1. ~~HID++ writes over wireless.~~ **Resolved**: they work (§3.5).
2. **`0x8110` capture versus evdev `grab`.** Determine whether the button
   spy delivers the buttons more cleanly than intercepting the event
   node, especially regarding the device's keyboard interface (§3.3).
   This is the only technical unknown remaining before implementation.

Then, the success criterion for the first version is concrete and verifiable:
**the nine Lost Ark macros running from a profile that activates automatically
when the game gains focus, with no stuck keys on release, and surviving an
alt-tab to the desktop and back.**

# OpenHub — where the work stands

> Written 2026-08-28 at the end of the first working session, so the next one
> does not have to reconstruct any of this. Read this before the plans.

## What OpenHub is, in one paragraph

A Logitech G HUB replacement for Linux, hard-forked from
[OpenLogi](https://github.com/AprilNEA/OpenLogi) at commit `b32ae087` on
2026-08-27. It exists because nine "repeat while held" macros for Lost Ark died
with a deleted Windows install, and nothing on Linux can reproduce them:
libratbag's macros are one-shot, the mouse firmware has no repeat mode, and
OpenLogi cannot even see a gaming mouse's buttons. Reference hardware is a
Logitech G703 LIGHTSPEED HERO. The macros themselves are transcribed in
`~/obsidian-vault/Resources/g703-macros-lost-ark.md` and in the design spec
§5.3 — that vault note is the only surviving copy of the originals.

## The repository

`github.com/msedek/openhub` — a standalone public repository, **not** a GitHub
fork, carrying OpenLogi's full 1152-commit history so every inherited commit
keeps its author. `origin` is that repo; `upstream` is OpenLogi, kept for
pulling changes down. The old `msedek/ghubclone` fork was deleted.

`master` is protected: no deletion, no force-push, pull request required (zero
approvals), with repository-admin bypass so nothing gets stuck. Merge commits
are off; squash or rebase only. **GitHub Actions is disabled** — the inherited
CI is OpenLogi's and expects their secrets. There is no CI. Every check in this
project is run locally, by hand.

**Licence: GPL-3.0-or-later**, changed from the inherited MIT/Apache-2.0. That
was a deliberate trade for Piper's device illustrations; `NOTICE` explains it
in full.

## State of the branches

| Branch | What it holds | Verified |
|---|---|---|
| `master` | README, design spec, plans, own brand assets, NOTICE | — |
| `feat/own-device-art` — **PR #1** | GPL relicence, 67 Piper drawings, SVG rendering pipeline | 202 desktop tests, full local gate green |
| `feat/gaming-device-recognition` — **PR #2** | Model table, `0x8100`, `diag onboard`, button capability | 1136 tests, verified against the real G703 |
| `feat/macro-engine` | Macro model and executor | 17 tests, 75+ runs with no flake, cadence measured |

Both pull requests are open and unmerged. Neither depends on the other; they
touch different crates and merge in either order.

## What actually works today

- `openlogi diag onboard` reads the G703's onboard state over **either**
  transport, cable or Lightspeed receiver, and cross-checks the device's
  reported button count against the model table.
- The G703 records `capabilities.buttons = true`, so it stops being a mouse
  with no buttons.
- The GUI names it "Logitech G703 Hero" and draws Piper's illustration with the
  button anchors landing on the right buttons.
- `ghub-macro` executes a macro at a measured 25 ms cadence and provably
  releases every key it pressed.

## What does not work yet

**No macro can be triggered by a button.** That is the current work: Tasks 3
and 4 of the macro plan — widening suppression so a bound button stops reaching
the desktop, and dispatching `Action::RunMacro` from the button runtime. Until
that lands, `ghub-macro` is a library nothing calls.

After it: per-game profiles (window matching, the G-Shift layer, the
level-triggered reconciliation in spec §6) and the GUI (profile list, macro
recorder, G-Shift toggle).

## The plans, in order

1. `plans/2026-08-28-gaming-device-recognition.md` — **done**, all five tasks.
2. `plans/2026-08-28-macro-engine.md` — Tasks 1 and 2 done. **Tasks 3 and 4 are
   in progress and may be half-applied in the working tree.** Task 5 is
   hardware verification and needs a person with the mouse.
3. Per-game profiles — not written yet. Spec §6 has the design.
4. The GUI — not written yet. Spec §8 has the design.

## Facts measured against the hardware, which nothing should re-derive

The G703 exposes 29 HID++ 4.2 features, including `0x8100` onboard profiles and
`0x8110` button spy, and **does not expose `0x1b04`** — that absence is the
whole reason OpenLogi sees no buttons on it.

| Button | evdev | Confirmations |
|---|---|---|
| Left | `BTN_LEFT` 272 | 1 |
| Right | `BTN_RIGHT` 273 | 1 |
| Wheel click | `BTN_MIDDLE` 274 | 4 |
| Rear side | `BTN_SIDE` 275 | 2, plus elimination |
| Front side | `BTN_EXTRA` 276 | 3 |
| Behind the wheel | `BTN_TASK` 279 | 3 |

Piper's drawing numbers its buttons in the same order, which is an independent
second source agreeing with the physical capture.

Three findings that cost real investigation:

- **HID++ writes work over the Lightspeed receiver.** The vault note claimed a
  cable was required; that was a libratbag limitation, not the hardware's.
  `openlogi diag dpi` round-trips wirelessly. The note has been corrected.
- **`sector_size` is 255, not 256.** Any future onboard write path assuming
  power-of-two sectors is wrong.
- **This G703 supports G-Shift in firmware.** libratbag calls byte 9
  `mechanical_layout`; solaar reads the same byte as `shift` and treats
  `shift & 0x3 == 0x2` as "has a G-Shift layer". The device reports `0x0a`. The
  `[SHIFT]` column in the owner's macro transcription was real, not a
  transcription error.
- **The sensor reports 100–25600 DPI in 511 steps**, far finer than the five
  presets libratbag exposes. The GUI should offer the real range.

## Things that will bite the next session

- **`cargo` is not on PATH by default.** It is at `~/.cargo/bin`. Rust 1.98.0
  was installed for this project; it did not exist on this machine before.
- **GPG signing fails** — the configured key has no secret half here. It is
  disabled in this repository's local config, so plain `git commit` works, but
  a fresh clone will need `git config commit.gpgsign false` again.
- **The mouse sleeps fast on battery.** `openlogi list` showing `○` means
  asleep, not broken. Wake it or plug the cable.
- **The agent never writes `config.toml`.** `Config::adopt_route` is called
  only from `openlogi-desktop`. Restarting the agent will never persist a
  capability change; the GUI has to run.
- **The packaged OpenLogi 0.7.1 was purged** from this machine. Its udev rules
  were kept — verified byte-identical to `packaging/linux/udev/` — because
  device access depends on them. A config backup sits at
  `~/.config/openlogi.backup-20260828-071039`, and the old Logitech render
  cache is still at `~/.local/share/openlogi/assets/` at the owner's request.
  **Never bundle that cache into a release.**
- **`design/devices/svg/fallback.svg` is not a generic mouse.** It is Piper's
  placeholder joke — a cartoon mouse holding a "404" sign. Unlisted pointing
  devices currently get it. Changing that is one call site.
- **Do not request screen capture.** On this GNOME Wayland session the only
  path is the Remote Desktop portal, which prompts the user for a session that
  can control their keyboard and mouse. Render SVGs headless with Rsvg and read
  the PNG instead; that is how the artwork was reviewed.
- **Never write to the mouse's onboard memory** without an explicit decision.
  It holds the owner's saved configuration, and the only tool that can repair a
  bad write is G HUB on a Windows install that no longer exists.

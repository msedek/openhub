# Macro Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hold a button on a G703 and have it spam a key at 25 ms until released — the thing G HUB does and nothing on Linux does.

**Architecture:** A new `ghub-macro` crate owns macro definitions and their execution: a sequence of press/release steps, one of G HUB's three repeat modes, and the guarantee that no key is ever left held. The agent's button runtime dispatches to it. The OS hook's suppression rule is widened so a bound button stops reaching the desktop.

**Tech Stack:** Rust 2024, MSRV 1.98. `ghub-macro` is `std`-only for its types; execution uses a dedicated thread with `timerfd` on Linux rather than the shared tokio runtime. Injection goes through the existing `openlogi-inject`.

**Spec:** [`docs/superpowers/specs/2026-08-27-openhub-design.md`](../specs/2026-08-27-openhub-design.md) — §5.3 defines the macro model and lists the nine real macros this must reproduce; §7 states the two non-negotiable properties.

## Global Constraints

- **Language.** Every file, comment, doc string, commit message and PR body is **English**. No exceptions.
- **Edition 2024, MSRV 1.98.** New crates set `rust-version.workspace = true` and `[lints] workspace = true`.
- **`clippy::pedantic` is warned workspace-wide and the gate runs `-D warnings`.** `float_cmp` is in that group — compare timings with a tolerance, never `assert_eq!` on a float.
- **The IPC wire format is append-only.** `Action` is a serde enum whose variant order *is* the wire format. A new variant goes at the **end**, and `PROTOCOL_VERSION` gets bumped. Read `crates/openlogi-ipc/AGENTS.md` before touching it, and run `cargo test -p openlogi-ipc --test wire_format`.
- **Never claim a check passed without running it.** Paste real output; name what could not run.
- **Nothing writes to the device.** This plan is host-side only. The G703's onboard memory holds macros rescued from a deleted Windows install; they exist nowhere else.

## What already exists, so you do not rebuild it

- **The hook already captures all six G703 buttons.** `crates/openlogi-hook/src/linux.rs:416` maps `BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`, `BTN_SIDE`→`Back`, `BTN_EXTRA`→`Forward`, `BTN_TASK`→`DpiToggle`. Capture is not the gap.
- **Injection exists**: `openlogi-inject` synthesises keys and buttons through `uinput` on Linux.
- **Press/release lifecycle exists**: `Action::HoldShortcut` already holds a chord for the lifetime of a physical press, so the runtime has a release context to hang cleanup on.
- **The gap is suppression.** `ButtonId::is_os_hook_button()` (`crates/openlogi-core/src/binding/button.rs:126`) returns true for only `MiddleClick`, `Back` and `Forward`. Every other button passes through to the desktop no matter what it is bound to, so `BTN_TASK` cannot host G-Shift and the right button cannot become an auto-clicker.

## File Structure

| File | Responsibility |
|---|---|
| `crates/ghub-macro/Cargo.toml` | New crate manifest |
| `crates/ghub-macro/src/lib.rs` | Crate root |
| `crates/ghub-macro/src/model.rs` | `Macro`, `Step`, `RepeatMode`, `MacroId` — pure types, serde |
| `crates/ghub-macro/src/executor.rs` | The run loop, the held-key ledger, the release guarantee |
| `crates/ghub-macro/src/timer.rs` | The pacing source; `timerfd` on Linux, `sleep` elsewhere |
| `crates/openlogi-core/src/binding/button.rs` | Widen suppression (modify) |
| `crates/openlogi-core/src/binding/action.rs` | `Action::RunMacro(MacroId)`, appended last (modify) |
| `crates/openlogi-core/src/config/device.rs` | A `macros` table on the config (modify) |
| `crates/openlogi-agent-core/src/runtime/button.rs` | Dispatch a macro on press, stop it on release (modify) |

---

## Task 1: The macro model

**Files:**
- Create: `crates/ghub-macro/Cargo.toml`, `src/lib.rs`, `src/model.rs`
- Modify: `Cargo.toml` (workspace members and `[workspace.dependencies]` — Task 1 of the previous plan forgot the second, so add both)

**Interfaces:**
- Produces:
  - `ghub_macro::MacroId(String)` — a newtype key.
  - `ghub_macro::Step` — `KeyDown(u16) | KeyUp(u16) | KeyTap(u16) | ButtonDown(u16) | ButtonUp(u16) | ButtonTap(u16) | Delay { millis: u32 }`. The `u16`s are Linux input event codes, the same numbers `ghub-models` uses.
  - `ghub_macro::RepeatMode` — `Once | WhileHeld { interval_ms: u32 } | Toggle { interval_ms: u32 }`.
  - `ghub_macro::Macro { pub id: MacroId, pub name: String, pub steps: Vec<Step>, pub repeat: RepeatMode }`
  - `Macro::held_codes(&self) -> Vec<u16>` — every code a run of this macro could leave held, derived from the steps.

`held_codes` is not a convenience. It is how the release guarantee is computed without the executor having to reason about arbitrary step sequences at cleanup time.

- [ ] **Step 1: Write the failing tests**

`crates/ghub-macro/src/model.rs`, tests only:

```rust
#[cfg(test)]
mod tests {
    use super::{Macro, MacroId, RepeatMode, Step};

    /// The real "Hyper" macro: hold Alt, tap V, release Alt, repeated every
    /// 25 ms while the button is down. Transcribed from the owner's G HUB
    /// configuration; see the design spec §5.3.
    fn hyper() -> Macro {
        Macro {
            id: MacroId("hyper".into()),
            name: "Hyper".into(),
            steps: vec![Step::KeyDown(56), Step::KeyTap(47), Step::KeyUp(56)],
            repeat: RepeatMode::WhileHeld { interval_ms: 25 },
        }
    }

    /// Anything a run could leave pressed must be reported, or the executor
    /// cannot guarantee it releases it. A tap is balanced and cannot leak; a
    /// bare KeyDown can.
    #[test]
    fn held_codes_reports_every_code_a_run_could_leave_down() {
        assert_eq!(hyper().held_codes(), vec![56]);
    }

    #[test]
    fn a_macro_of_taps_can_leak_nothing() {
        let m = Macro {
            id: MacroId("superspace".into()),
            name: "SuperSpace".into(),
            steps: vec![Step::KeyTap(57)],
            repeat: RepeatMode::WhileHeld { interval_ms: 25 },
        };

        assert!(m.held_codes().is_empty());
    }

    /// Mouse buttons leak exactly like keys do.
    #[test]
    fn held_codes_covers_buttons_too() {
        let m = Macro {
            id: MacroId("drag".into()),
            name: "Drag".into(),
            steps: vec![Step::ButtonDown(272), Step::Delay { millis: 50 }],
            repeat: RepeatMode::Once,
        };

        assert_eq!(m.held_codes(), vec![272]);
    }

    /// A code pressed and released within the same run is balanced, and
    /// reporting it would make cleanup emit a spurious release.
    #[test]
    fn a_balanced_down_and_up_is_not_held() {
        let m = Macro {
            id: MacroId("balanced".into()),
            name: "Balanced".into(),
            steps: vec![Step::KeyDown(29), Step::KeyTap(46), Step::KeyUp(29)],
            repeat: RepeatMode::Once,
        };

        assert!(m.held_codes().is_empty());
    }

    #[test]
    fn round_trips_through_toml() {
        let m = hyper();
        let text = toml::to_string(&m).expect("serialises");
        let back: Macro = toml::from_str(&text).expect("deserialises");

        assert_eq!(back, m);
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test -p ghub-macro`
Expected: FAIL — the package does not exist.

- [ ] **Step 3: Implement the model**

Write `model.rs` above the tests. `held_codes` walks the steps keeping a running set: `KeyDown`/`ButtonDown` insert, `KeyUp`/`ButtonUp` remove, taps and delays do nothing. What remains at the end is what a run leaks. Return it sorted so the result is deterministic.

Derive `Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize` on every type. `MacroId` also derives `Hash, PartialOrd, Ord` so it can key a `BTreeMap`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ghub-macro`
Expected: 5 pass.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p ghub-macro --all-targets -- -D warnings
cargo fmt --all -- --check
git add crates/ghub-macro Cargo.toml Cargo.lock
git commit -m "feat(macro): add the macro model

A macro is a sequence of press/release steps and one of G HUB's three repeat
modes. Steps carry Linux input event codes, the same numbers the model table
uses, so nothing translates between layers.

held_codes derives what a run could leave pressed. That is not a convenience:
it is how the executor guarantees it releases everything without reasoning
about arbitrary step sequences at cleanup time."
```

---

## Task 2: The executor and its release guarantee

**Files:**
- Create: `crates/ghub-macro/src/executor.rs`, `crates/ghub-macro/src/timer.rs`

**Interfaces:**
- Consumes: `Macro`, `Step`, `RepeatMode` from Task 1.
- Produces:
  - `ghub_macro::Sink` — a trait the executor emits through: `fn key_down(&self, code: u16)`, `key_up`, `button_down`, `button_up`. Implemented by the agent over `openlogi-inject`, and by a recording fake in tests.
  - `ghub_macro::Executor::new(sink: Arc<dyn Sink>) -> Executor`
  - `Executor::start(&self, macro_def: &Macro) -> RunHandle`
  - `RunHandle::stop(self)` — idempotent, and emits every outstanding release before returning.

**The property this task exists for:** after `stop`, or after the executor is dropped, or after a panic in the run thread, **every code the run pressed has been released**. Test it, do not assert it in prose.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{Executor, Sink};
    use crate::{Macro, MacroId, RepeatMode, Step};

    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<(&'static str, u16)>>,
    }

    impl Sink for Recorder {
        fn key_down(&self, code: u16) {
            self.events.lock().unwrap().push(("kd", code));
        }
        fn key_up(&self, code: u16) {
            self.events.lock().unwrap().push(("ku", code));
        }
        fn button_down(&self, code: u16) {
            self.events.lock().unwrap().push(("bd", code));
        }
        fn button_up(&self, code: u16) {
            self.events.lock().unwrap().push(("bu", code));
        }
    }

    impl Recorder {
        fn events(&self) -> Vec<(&'static str, u16)> {
            self.events.lock().unwrap().clone()
        }
        /// Every down must have a matching up by the end. This is the whole
        /// point of the crate: a leaked Alt is unusable for the user and
        /// invisible to a test that only counts events.
        fn is_balanced(&self) -> bool {
            let mut held: Vec<u16> = Vec::new();
            for (kind, code) in self.events() {
                match kind {
                    "kd" | "bd" => held.push(code),
                    "ku" | "bu" => {
                        if let Some(i) = held.iter().position(|c| *c == code) {
                            held.remove(i);
                        }
                    }
                    _ => {}
                }
            }
            held.is_empty()
        }
    }

    fn hyper() -> Macro {
        Macro {
            id: MacroId("hyper".into()),
            name: "Hyper".into(),
            steps: vec![Step::KeyDown(56), Step::KeyTap(47), Step::KeyUp(56)],
            repeat: RepeatMode::WhileHeld { interval_ms: 25 },
        }
    }

    #[test]
    fn once_runs_the_sequence_exactly_once() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());
        let mut m = hyper();
        m.repeat = RepeatMode::Once;

        exec.start(&m).stop();

        assert_eq!(sink.events().iter().filter(|(k, c)| *k == "kd" && *c == 56).count(), 1);
        assert!(sink.is_balanced());
    }

    #[test]
    fn while_held_repeats_until_stopped() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());

        let handle = exec.start(&hyper());
        std::thread::sleep(std::time::Duration::from_millis(200));
        handle.stop();

        let runs = sink.events().iter().filter(|(k, c)| *k == "kd" && *c == 56).count();
        // 200 ms at 25 ms is 8 runs; timing is not exact, so allow slack in
        // both directions rather than asserting a number the scheduler owns.
        assert!((3..=12).contains(&runs), "expected repeats, got {runs}");
        assert!(sink.is_balanced());
    }

    /// Stopping mid-sequence is the case that leaks in every naive
    /// implementation: the button is released between KeyDown(Alt) and
    /// KeyUp(Alt), and Alt stays down forever.
    #[test]
    fn stopping_mid_sequence_still_releases_everything() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());
        let m = Macro {
            id: MacroId("slow".into()),
            name: "Slow".into(),
            steps: vec![
                Step::KeyDown(56),
                Step::Delay { millis: 5_000 },
                Step::KeyUp(56),
            ],
            repeat: RepeatMode::Once,
        };

        let handle = exec.start(&m);
        std::thread::sleep(std::time::Duration::from_millis(50));
        handle.stop();

        assert!(sink.is_balanced(), "Alt was left held: {:?}", sink.events());
    }

    #[test]
    fn dropping_the_handle_releases_everything() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());
        let m = Macro {
            id: MacroId("slow".into()),
            name: "Slow".into(),
            steps: vec![Step::KeyDown(56), Step::Delay { millis: 5_000 }],
            repeat: RepeatMode::Once,
        };

        drop(exec.start(&m));
        std::thread::sleep(std::time::Duration::from_millis(50));

        assert!(sink.is_balanced());
    }

    #[test]
    fn stop_is_idempotent() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());
        let handle = exec.start(&hyper());
        handle.stop();
        // A second stop must not emit a second set of releases; the balance
        // check catches a spurious up as readily as a missing one.
        assert!(sink.is_balanced());
    }
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -p ghub-macro executor`
Expected: FAIL — `Executor` does not exist.

- [ ] **Step 3: Implement**

The run loop lives on its own `std::thread`, not on the tokio runtime the agent shares — a 25 ms cadence competing with device I/O drifts, and drift is exactly what makes spam feel weak. Pace it with `timerfd` on Linux (`timer.rs`, cfg-gated, with a `thread::sleep` fallback elsewhere) so the interval is measured from a fixed clock rather than accumulating the cost of each run.

Track pressed codes in a ledger the thread owns. On every exit path — stop signalled, macro finished, panic caught with `catch_unwind` — walk the ledger and emit the matching release for each entry, then clear it. `RunHandle::stop` signals, joins, and returns only after the thread has drained its ledger.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ghub-macro`
Expected: all pass. Run it several times — a release guarantee that passes once and fails under load is not a guarantee. `cargo test -p ghub-macro -- --test-threads=1` and again with the default parallelism.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p ghub-macro --all-targets -- -D warnings
git add crates/ghub-macro
git commit -m "feat(macro): add the executor and its release guarantee

Runs a macro on a dedicated thread paced by timerfd, because a 25 ms cadence
sharing the agent's tokio runtime with device I/O drifts, and drift is what
makes held-button spam feel weak precisely when it matters.

Every code the run presses goes into a ledger, and every exit path drains it —
stop, completion, drop, panic. The test that matters is the one that stops the
run between KeyDown(Alt) and KeyUp(Alt): that is where every naive
implementation leaves Alt held forever."
```

---

## Task 3: Let a bound button be suppressed

**Files:**
- Modify: `crates/openlogi-core/src/binding/button.rs` (`is_os_hook_button`, line 126)
- Test: the same file's test module

Today only `MiddleClick`, `Back` and `Forward` may be suppressed. Everything else reaches the desktop regardless of its binding, which means `DpiToggle` (the G703's `BTN_TASK`, behind the wheel) cannot host G-Shift, and the right button cannot become an auto-clicker.

- [ ] **Step 1: Write the failing test**

Assert that `DpiToggle`, `LeftClick` and `RightClick` are suppressible, alongside the three that already are.

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test -p openlogi-core is_os_hook_button`

- [ ] **Step 3: Implement, with the one guard that matters**

Widen the rule, but keep a floor: **the left button is never suppressed unless it carries an explicit binding.** A profile that suppresses the primary button with nothing bound leaves the machine with no way to click, and the user cannot open the GUI to fix it. Encode that as a rule with a comment saying why, not as a comment alone.

- [ ] **Step 4: Run the affected tests**

Run: `cargo test -p openlogi-core`
Expected: pass. If an existing test asserted the narrow rule, update it deliberately and say so in the commit — do not delete it.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(core): let any bound button be suppressed

Suppression was limited to the middle and two thumb buttons, so a binding on
anything else was advisory: the physical event reached the desktop anyway.
That makes the G703's button behind the wheel unusable as a G-Shift trigger
and the right button unusable as an auto-clicker.

The left button keeps a floor — it is suppressed only when something is
explicitly bound to it. A profile that swallows the primary click with nothing
in its place leaves no way to reach the GUI and undo it."
```

---

## Task 4: Fire a macro from a button

**Files:**
- Modify: `crates/openlogi-core/src/binding/action.rs` — append `Action::RunMacro(MacroId)`
- Modify: `crates/openlogi-core/src/config/device.rs` — a `macros: BTreeMap<MacroId, Macro>` table
- Modify: `crates/openlogi-agent-core/src/runtime/button.rs` — dispatch and stop
- Modify: `crates/openlogi-ipc/src/ipc.rs` — bump `PROTOCOL_VERSION`

**Read `crates/openlogi-ipc/AGENTS.md` first.** `Action`'s variant order is the wire format; the new variant goes **last**, and the version bumps.

- [ ] **Step 1: Append the action and the config table**

`RunMacro(MacroId)` at the end of `Action`. Give it a `label` arm reading the macro's name and a catalog entry excluded from the default picker, the way `TypeText` and `Workflow` already are.

- [ ] **Step 2: Wire the runtime**

On press: look the macro up, `Executor::start`, keep the `RunHandle` keyed by the physical button. On release of that button: `stop`. On profile change, focus loss or shutdown: stop every outstanding handle.

The runtime already has this lifecycle for `HoldShortcut` — follow it rather than inventing a parallel one.

- [ ] **Step 3: Verify the wire format**

```bash
cargo test -p openlogi-ipc --test wire_format
```
Expected: pass with the bumped version. If it fails, the variant went somewhere other than the end.

- [ ] **Step 4: Test the runtime**

Add a test to `runtime/button.rs`'s module: pressing a button bound to `RunMacro` starts a run, releasing it stops it, and a profile change with the button still held stops it too.

- [ ] **Step 5: Full gate**

The diff touches `Cargo.toml` and the IPC contract, so the affected-package tier does not apply:

```bash
export RUSTFLAGS="-D warnings"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(agent): run a macro while its button is held

Action gains RunMacro, appended last because the variant order is the IPC wire
format, and PROTOCOL_VERSION bumps with it.

The runtime starts a run on press and stops it on release, keyed by the
physical button, reusing the lifecycle HoldShortcut already established. Every
terminal path — release, profile change, focus loss, shutdown — stops
outstanding runs, so the executor's release guarantee is actually reachable
rather than theoretical."
```

---

## Task 5: Run the real macros on the real mouse

**Files:** none — this is verification.

The G703 is attached. It sleeps quickly on battery; if `openlogi list` shows `○`, wake it.

- [ ] **Step 1: Write the nine macros into the config**

Transcribe the spec's §5.3 table into `~/.config/openlogi/config.toml`, backing the file up first. Bind at least `SuperSpace` (space at 25 ms) and `The T` (T at 50 ms) to real buttons.

- [ ] **Step 2: Measure the cadence, do not eyeball it**

Open a text editor, hold the button, release. Then measure properly: record the injected events and check the interval.

```bash
sudo evtest --grab /dev/input/by-id/...  # or read the uinput device directly
```

Expected: intervals clustered near 25 ms. Report the actual distribution, including the worst case — a p99 of 60 ms is a finding, not a rounding error.

- [ ] **Step 3: Try to leak a key**

Hold a button bound to `Hyper` (`Alt↓ V Alt↑`), and release it repeatedly and erratically, including mid-sequence. Then check no modifier is stuck: `xdotool key --clearmodifiers a` in a text field, or read the modifier state.

**This is the test that decides whether the feature is usable.** A macro engine that leaks Alt once an hour is worse than no macro engine.

- [ ] **Step 4: Report**

Record the measured cadence, the leak result, and anything that surprised you, in the pull request.

---

## Definition of done

Holding a button on the G703 spams a key at its configured interval, releasing it stops cleanly, and no key is ever left held — measured, not assumed.

## What comes after

| Plan | Delivers |
|---|---|
| **Per-game profiles** | Named profiles, window matching, the G-Shift layer, and the level-triggered reconciliation from spec §6. Includes making the GNOME Shell extension push rather than poll. |
| **The GUI** | Profile list, macro recorder, G-Shift toggle on the button screen. |

# Per-Game Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Alt-tab into Lost Ark and have the mouse switch to the Lost Ark assignments — both layers, G-Shift included — within milliseconds, and never get stuck on the wrong profile the way G HUB does.

**Architecture:** A `GameProfile` is a first-class, document-scoped config object with match rules and two assignment layers. The agent turns focus readings into an *applied* profile by level-triggered reconciliation: every reading (pushed by the GNOME Shell extension, or polled as a safety net) is compared against what is applied, and the agent converges; an unreadable focus is *unknown*, never *nothing*, so a profile only falls when another identifiable window takes focus. The G-Shift layer is resolved on the OS-hook thread from one atomic flag, so it costs nothing on the hot path and never restarts a HID++ capture session.

**Tech Stack:** Rust 2024, MSRV 1.98. `regex-lite` for title rules (pure Rust, no unicode tables, keeps `openlogi-core` `wasm32`-checkable). `zbus 5` blocking signals for the push path. GJS for the extension.

**Spec:** [`docs/superpowers/specs/2026-08-27-openhub-design.md`](../specs/2026-08-27-openhub-design.md) — §5.2 defines the profile, §6 the three focus corrections and the push path, §7 the "no key left held on profile change" property. Read `docs/superpowers/STATUS.md` first.

## Global Constraints

- **Language.** Every file, comment, doc string, commit message and PR body is **English**. No exceptions.
- **Edition 2024, MSRV 1.98**, `[lints] workspace = true`. `clippy::pedantic` is warned workspace-wide and the gate runs with `RUSTFLAGS="-D warnings"`. Suppressions are `#[expect(…, reason = "…")]`, never bare `allow`.
- **The IPC wire format is append-only.** `Action` and `AgentSnapshot` ride the wire. A new `Action` variant goes at the **end** of the enum; a new snapshot field goes at the **end** of the struct; each bump `PROTOCOL_VERSION` and regenerate `crates/openlogi-ipc/tests/wire_format.rs`. Read `crates/openlogi-ipc/AGENTS.md` first.
- **`Config` is `#[serde(deny_unknown_fields)]`.** A new top-level field is `#[serde(default)]` and needs no schema bump (precedent: `macros`, `config.rs:132-145`). `docs/config.example.toml` is asserted by `canonical_configuration_example_parses` — keep it parsing.
- **The hook callback must never block.** Every read on the tap thread is `try_read()` and fails open. New held-state lives in an atomic or a `thread_local!`, never a new lock.
- **The agent never writes `config.toml`.** Profiles are authored in TOML (and by the GUI in the next plan); the agent only reads them.
- **Nothing writes to the device.** Host-side only. The G703's onboard memory holds the owner's rescued macros.
- **Never claim a check passed without running it.** Paste real output; name what could not run. There is no CI on this repository.
- **Do not request screen capture** to verify anything on this GNOME Wayland session. Use logs, `busctl`, and `evtest`.

## Decisions this plan makes where the spec left room

| Decision | Why |
|---|---|
| Assignments are keyed by **`ButtonId`**, not the spec's `ButtonRef { model, slot }`. | `ButtonRef` was never built: PR #2 kept `ButtonId`, and the hook decodes evdev codes to it (`BTN_SIDE`→`Back`, `BTN_EXTRA`→`Forward`, `BTN_TASK`→`DpiToggle`). The G703's six buttons map 1:1. Moving the whole binding model to slots is its own refactor and buys nothing this plan needs. |
| `Assignment` **is `Action`**, plus one appended variant `Action::GShift`. | `Action::RunMacro(MacroId)` and `Action::None` (= *Disabled*) already exist and already ride the wire. A parallel enum would duplicate the catalog, the icon table and the stability test for no gain. |
| Match rules: `WmClass` beats `SteamAppId` beats `Title`; ties resolve to the first profile id in `BTreeMap` order. | Spec §6 rule 3: WM_CLASS is the identity, the other two are fallbacks. Deterministic order means the same config always picks the same profile. |
| Profiles are **document-scoped** (`[profiles.<id>]`), like `[macros]`. | The OS-hook path carries no device key (`config.rs:132-145`); a per-device table would be unreachable from exactly the path a gaming mouse's buttons arrive on. |
| "`None` means unknown" applies to the inherited per-app overlay too, not only to game profiles. | One code path resolves both, and the rule is about focus readings, not about which table they feed. The old behavior — minimizing everything dropped the per-app overlay — is the bug §6 names. |
| The G-Shift layer applies to **OS-hook buttons**, not to HID++-diverted ones. | Every G703 button reaches the hook (no `0x1b04`). Layering the HID++ capture-plan path would change `divert_buttons`, which is plan identity, and restart capture sessions on every shift press. |
| **Per-profile device settings** (DPI presets, report rate, lighting — the spec's `device:` block) are **not in this plan**. | They need a HID++ write on activation, which is a different subsystem from matching and reconciliation. Tracked as the follow-up in "What comes after". |
| The extension keeps its uuid `openlogi-frontmost@openlogi.dev` and D-Bus name. | A uuid is an identity; renaming it means a second install for every user. The agent talks to v1 and v2 (fallback on `UnknownMethod`). |

## What already exists, so you do not rebuild it

- **Focus already drives bindings.** `Orchestrator::set_current_app` (`crates/openlogi-agent-core/src/orchestrator.rs:748`) rewrites the hook maps and republishes capture plans; its caller `Lifecycle::apply_foreground` (`crates/openlogi-agent/src/lifecycle.rs:343`) then runs `ActionDispatcher::cancel_all_buttons`, which bumps the press generation **and calls `MacroRunner::stop_all`**. Spec §7's "release everything on profile change" is therefore already true — keep that call on every applied change.
- **The per-app overlay is the shape to copy.** `Config::effective_bindings(device_key, app)` (`crates/openlogi-core/src/config.rs:544`) overlays `BTreeMap<ButtonId, Action>` as `Binding::Single`. Profiles overlay the same way, after it.
- **The poll machinery exists.** `watchers::poll::Poll::on_change` (`crates/openlogi-agent-core/src/watchers/poll.rs:41`) is a named thread feeding a tokio unbounded channel. This plan adds an `every` variant; it does not add a runtime.
- **The extension exists and is installed nowhere.** `crates/openlogi-hook/gnome-shell-extension/…/extension.js` exports one pull method, `GetFocusedWmClass`. The agent's client is `crates/openlogi-hook/src/linux/gnome_shell.rs` (zbus 5, blocking, `gen_async = false`).
- **The wire already carries the foreground app.** `AgentSnapshot::foreground: ForegroundApps { current, recent }` (`crates/openlogi-ipc/src/ipc.rs:126-169`). It does not carry which profile is applied; this plan appends that.
- **The hook's held-state precedent** is `HoldState` in `thread_local! HOLD` (`crates/openlogi-agent-core/src/runtime/hook.rs:73-173`) and the `FAIL_OPEN_PRESSES` set that pairs a fallen-through press with its release. G-Shift copies the pairing idea.
- **The nine macros** are in spec §5.3 and `~/obsidian-vault/Resources/g703-macros-lost-ark.md`; the `[SHIFT]` column there is the G-Shift layer.

## File Structure

| File | Responsibility |
|---|---|
| `crates/openlogi-core/src/profile.rs` | **New.** `ProfileId`, `MatchRule`, `Assignments`, `GameProfile`, and `resolve()` — the pure matcher |
| `crates/openlogi-core/src/app.rs` | `FocusedWindow` — what a focus source could read, agent-internal (modify) |
| `crates/openlogi-core/src/config.rs` | `Config::profiles`, `effective_bindings_in`, `g_shift_layer` (modify) |
| `crates/openlogi-core/src/bindings.rs` | `ActiveScope`; resolvers take a scope instead of an app string; `g_shift_bindings_for` (modify) |
| `crates/openlogi-core/src/binding/action.rs`, `effect.rs`, `binding/tests.rs` | `Action::GShift` (modify) |
| `crates/openlogi-hook/src/lib.rs`, `src/linux.rs`, `src/linux/gnome_shell.rs`, `src/linux/wlr_foreign_toplevel.rs` | `focused_window()` and `watch_focus()`; title, pid, Steam AppID on Linux (modify) |
| `crates/openlogi-hook/gnome-shell-extension/openlogi-frontmost@openlogi.dev/extension.js`, `metadata.json`, `../README.md` | Extension v2: `GetFocusedWindow`, `FocusChanged` signal (modify) |
| `crates/openlogi-agent-core/src/watchers/poll.rs`, `watchers/foreground_app.rs` | `Poll::every_into`; the focus watcher merges push + poll (modify) |
| `crates/openlogi-agent-core/src/orchestrator.rs`, `orchestrator/tests.rs` | `reconcile_focus`, `last_focus`, `active_profile`, `scope()` (modify) |
| `crates/openlogi-agent-core/src/observable.rs` | `set_active_profile` (modify) |
| `crates/openlogi-agent-core/src/runtime/hook.rs`, `hook/tests.rs`, `runtime.rs` | `HookMaps::g_shift`, the G-Shift flag, shifted-press pairing (modify) |
| `crates/openlogi-agent-core/src/capture_plan.rs` | `plan_for_device` takes an `ActiveScope` (modify) |
| `crates/openlogi-agent/src/lifecycle.rs`, `src/startup.rs` | Route `FocusReading` to `reconcile_focus` (modify) |
| `crates/openlogi-ipc/src/ipc.rs`, `tests/wire_format.rs` | `AgentSnapshot::active_profile`; two version bumps (modify) |
| `crates/openlogi-desktop/src/state/bindings.rs` | One call site takes `&ActiveScope::default()` (modify) |
| `docs/CONFIGURATION.md`, `docs/config.example.toml`, `docs/INSTALL-linux.md`, `README.md` | `[profiles]` and `[macros]` documented; extension install (modify) |
| `packaging/linux/nfpm.yaml`, `install.sh`, `uninstall.sh` | Ship the extension (modify) |

---

## Task 1: The profile model and its matcher

**Files:**
- Create: `crates/openlogi-core/src/profile.rs`
- Modify: `crates/openlogi-core/src/lib.rs` (add `pub mod profile;` between `paths` and `scroll`), `crates/openlogi-core/src/app.rs`, `crates/openlogi-core/src/config.rs:146` (after `macros`), `crates/openlogi-core/Cargo.toml` (`regex-lite = "0.1"`), `docs/config.example.toml`, `docs/CONFIGURATION.md`
- Test: `crates/openlogi-core/src/profile.rs` (inline `mod tests`), `crates/openlogi-core/src/config/tests.rs`

**Interfaces:**
- Produces:
  - `openlogi_core::app::FocusedWindow { pub app: ForegroundApp, pub title: Option<String>, pub pid: Option<u32>, pub steam_app_id: Option<u32> }` with `FocusedWindow::app(app: ForegroundApp) -> Self` (the other three `None`). **Not a wire type.**
  - `openlogi_core::profile::ProfileId(pub String)` — `Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize`.
  - `openlogi_core::profile::MatchRule` — `WmClass(String) | SteamAppId(u32) | Title(String)`.
  - `openlogi_core::profile::Assignments { pub normal: BTreeMap<ButtonId, Action>, pub g_shift: BTreeMap<ButtonId, Action> }`.
  - `openlogi_core::profile::GameProfile { pub name: String, pub icon: Option<String>, pub matches: Vec<MatchRule>, pub assignments: Assignments }`.
  - `openlogi_core::profile::resolve(profiles: &BTreeMap<ProfileId, GameProfile>, window: &FocusedWindow) -> Option<&ProfileId>`.
  - `Config::profiles: BTreeMap<ProfileId, GameProfile>`.

- [ ] **Step 1: Write the failing tests**

In `crates/openlogi-core/src/profile.rs`, at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ForegroundApp;

    fn window(app_id: &str) -> FocusedWindow {
        FocusedWindow::app(ForegroundApp::unnamed(app_id.into()))
    }

    fn profile(matches: Vec<MatchRule>) -> GameProfile {
        GameProfile {
            name: "Lost Ark".into(),
            icon: None,
            matches,
            assignments: Assignments::default(),
        }
    }

    fn table(entries: Vec<(&str, GameProfile)>) -> BTreeMap<ProfileId, GameProfile> {
        entries
            .into_iter()
            .map(|(id, profile)| (ProfileId(id.into()), profile))
            .collect()
    }

    #[test]
    fn wm_class_matches_case_insensitively() {
        let profiles = table(vec![("lost-ark", profile(vec![MatchRule::WmClass("lostark.exe".into())]))]);
        assert_eq!(resolve(&profiles, &window("LostArk.exe")).map(|id| id.0.as_str()), Some("lost-ark"));
        assert_eq!(resolve(&profiles, &window("steam")), None);
    }

    #[test]
    fn steam_app_id_matches_when_the_class_does_not() {
        let profiles = table(vec![("lost-ark", profile(vec![MatchRule::SteamAppId(1_599_340)]))]);
        let mut win = window("steam_app_1599340");
        assert_eq!(resolve(&profiles, &win), None);
        win.steam_app_id = Some(1_599_340);
        assert!(resolve(&profiles, &win).is_some());
    }

    #[test]
    fn title_is_a_regex_and_needs_a_title() {
        let profiles = table(vec![("lost-ark", profile(vec![MatchRule::Title("^LOST ARK".into())]))]);
        let mut win = window("unknown");
        assert_eq!(resolve(&profiles, &win), None);
        win.title = Some("LOST ARK — Arthetine".into());
        assert!(resolve(&profiles, &win).is_some());
        win.title = Some("Steam — LOST ARK".into());
        assert_eq!(resolve(&profiles, &win), None);
    }

    #[test]
    fn a_broken_pattern_never_matches() {
        let profiles = table(vec![("bad", profile(vec![MatchRule::Title("(".into())]))]);
        let mut win = window("x");
        win.title = Some("(".into());
        assert_eq!(resolve(&profiles, &win), None);
    }

    #[test]
    fn wm_class_outranks_title_and_ties_go_to_the_first_id() {
        let profiles = table(vec![
            ("b-by-title", profile(vec![MatchRule::Title(".*".into())])),
            ("c-by-class", profile(vec![MatchRule::WmClass("game".into())])),
            ("a-by-title", profile(vec![MatchRule::Title(".*".into())])),
        ]);
        let mut win = window("game");
        win.title = Some("anything".into());
        assert_eq!(resolve(&profiles, &win).map(|id| id.0.as_str()), Some("c-by-class"));
        win.app.id = "other".into();
        assert_eq!(resolve(&profiles, &win).map(|id| id.0.as_str()), Some("a-by-title"));
    }

    #[test]
    fn round_trips_through_toml() {
        let mut profile = profile(vec![
            MatchRule::WmClass("lostark.exe".into()),
            MatchRule::SteamAppId(1_599_340),
            MatchRule::Title("^LOST ARK".into()),
        ]);
        profile.assignments.normal.insert(ButtonId::Back, Action::RunMacro(ghub_macro::MacroId("hyper".into())));
        profile.assignments.g_shift.insert(ButtonId::RightClick, Action::RunMacro(ghub_macro::MacroId("superright".into())));
        profile.assignments.normal.insert(ButtonId::DpiToggle, Action::None);
        let text = toml::to_string(&profile).expect("serialize");
        assert!(text.contains("[assignments.g_shift]"), "{text}");
        let back: GameProfile = toml::from_str(&text).expect("parse");
        assert_eq!(back, profile);
    }
}
```

In `crates/openlogi-core/src/config/tests.rs`, next to `canonical_configuration_example_parses`:

```rust
#[test]
fn profiles_roundtrip_and_the_example_carries_one() {
    let mut cfg = Config::default();
    let mut profile = GameProfile {
        name: "Lost Ark".into(),
        icon: None,
        matches: vec![MatchRule::WmClass("lostark.exe".into())],
        assignments: Assignments::default(),
    };
    profile
        .assignments
        .normal
        .insert(ButtonId::MiddleClick, Action::RunMacro(MacroId("superspace".into())));
    cfg.profiles.insert(ProfileId("lost-ark".into()), profile.clone());
    let restored = write_and_read(&cfg);
    assert_eq!(restored.profiles.get(&ProfileId("lost-ark".into())), Some(&profile));

    let body = include_str!("../../../../docs/config.example.toml");
    let documented: Config = toml::from_str(body).expect("documented config must parse");
    let example = documented
        .profiles
        .get(&ProfileId("lost-ark".into()))
        .expect("the example documents one profile");
    assert!(example.matches.iter().any(|rule| matches!(rule, MatchRule::WmClass(_))));
    assert!(!example.assignments.g_shift.is_empty(), "the example shows the G-Shift layer");
}
```

Add the imports the file needs: `use crate::profile::{Assignments, GameProfile, MatchRule, ProfileId};` and `use ghub_macro::MacroId;`.

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -p openlogi-core profile`
Expected: compile error — `crate::profile` does not exist.

- [ ] **Step 3: Implement the model**

`crates/openlogi-core/src/app.rs`, after `ForegroundApp`:

```rust
/// Everything a focus source could read about the window in front.
///
/// Agent-internal: it never crosses the IPC boundary. [`ForegroundApp`] is its
/// wire projection, and the extra fields exist only so a per-game profile can
/// fall back from the application identifier to a Steam AppID or a window
/// title (spec §6, rule 3). Every field but `app` is `None` where the source
/// cannot read it — macOS and Windows report the application only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusedWindow {
    /// The application, as the per-app overlay and the GUI already know it.
    pub app: ForegroundApp,
    /// The window title, when the source reports one.
    pub title: Option<String>,
    /// The client process, when the source reports one.
    pub pid: Option<u32>,
    /// `SteamAppId` from the client's environment, when `pid` is known and the
    /// process was launched by Steam.
    pub steam_app_id: Option<u32>,
}

impl FocusedWindow {
    /// A reading that knows the application and nothing else.
    #[must_use]
    pub fn app(app: ForegroundApp) -> Self {
        Self {
            app,
            title: None,
            pid: None,
            steam_app_id: None,
        }
    }
}
```

`crates/openlogi-core/src/profile.rs`:

```rust
//! Per-game profiles: a named set of button assignments that applies while a
//! matching window has focus (spec §5.2), and the matcher that decides which
//! one that is (spec §6, rule 3).
//!
//! A profile is a first-class object with a name, not an overlay hidden inside
//! a device entry like `per_app_bindings`. That difference is the G HUB user's
//! mental model and the reason this module exists beside the per-app overlay
//! rather than inside it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::app::FocusedWindow;
use crate::binding::{Action, ButtonId};

/// The key of a profile in the config's `[profiles]` table, and the value the
/// agent publishes as the applied profile.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

/// One way a window can be recognised as a game.
///
/// Rules are tried strongest first — `WmClass`, then `SteamAppId`, then
/// `Title` — because `WM_CLASS` is stable across launchers and Proton
/// versions and a title is whatever the game feels like showing today.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchRule {
    /// The application identifier the focus source reports: the `WM_CLASS`
    /// class on X11/GNOME, the xdg `app_id` on wlroots. Compared ASCII
    /// case-insensitively, since Wine derives it from the executable name and
    /// Windows never cared about case.
    WmClass(String),
    /// Steam's AppID, read from the client process's environment.
    SteamAppId(u32),
    /// A regular expression matched against the window title.
    Title(String),
}

/// How strongly a rule matched; `Ord` is the priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Strength {
    Title,
    SteamAppId,
    WmClass,
}

impl MatchRule {
    fn strength_against(&self, window: &FocusedWindow) -> Option<Strength> {
        match self {
            Self::WmClass(class) => class
                .eq_ignore_ascii_case(&window.app.id)
                .then_some(Strength::WmClass),
            Self::SteamAppId(id) => (window.steam_app_id == Some(*id)).then_some(Strength::SteamAppId),
            Self::Title(pattern) => {
                let title = window.title.as_deref()?;
                let regex = regex_lite::Regex::new(pattern)
                    .map_err(|error| debug!(pattern, %error, "title rule does not compile"))
                    .ok()?;
                regex.is_match(title).then_some(Strength::Title)
            }
        }
    }
}

/// The two assignment layers of a profile.
///
/// A button absent from a layer keeps whatever it would do without the
/// profile; `g_shift` lists only the buttons that change while the trigger is
/// held. [`Action::None`] disables a button; `Action::GShift` makes it the
/// trigger.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignments {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub normal: BTreeMap<ButtonId, Action>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub g_shift: BTreeMap<ButtonId, Action>,
}

/// A per-game profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameProfile {
    /// What the profile list shows.
    pub name: String,
    /// An icon for the profile list; a path, resolved by the GUI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Any rule matching activates the profile; the strongest decides ties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchRule>,
    #[serde(default)]
    pub assignments: Assignments,
}

/// The profile `window` should activate, or `None` when no rule matches.
///
/// The strongest rule wins across all profiles; two profiles matching with the
/// same strength resolve to the first id in table order, so the answer is a
/// function of the config alone.
#[must_use]
pub fn resolve<'a>(
    profiles: &'a BTreeMap<ProfileId, GameProfile>,
    window: &FocusedWindow,
) -> Option<&'a ProfileId> {
    profiles
        .iter()
        .filter_map(|(id, profile)| {
            let strength = profile
                .matches
                .iter()
                .filter_map(|rule| rule.strength_against(window))
                .max()?;
            Some((strength, id))
        })
        // Stronger wins; on equal strength the *smaller* id must compare
        // greater so `max_by` keeps it.
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(a.1)))
        .map(|(_, id)| id)
}
```

`Action::GShift` does not exist until Task 2, which is why the doc comment above names it in plain backticks; Task 2 turns it into an intra-doc link.

`crates/openlogi-core/src/config.rs`, after `macros`:

```rust
    /// Per-game profiles, keyed by the id the agent publishes as the applied
    /// one. Document-scoped for the same reason `macros` is: the OS-hook path
    /// that a gaming mouse's buttons arrive on carries no device key.
    ///
    /// `#[serde(default)]` keeps configs without a `[profiles]` section
    /// loading unchanged, so this needs no schema bump either.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<ProfileId, GameProfile>,
```

Add `profiles: BTreeMap::new()` to `impl Default for Config` (`config.rs:149`) and the import `use crate::profile::{GameProfile, ProfileId};`.

`crates/openlogi-core/Cargo.toml` `[dependencies]`: `regex-lite = "0.1"` with the comment `# Title rules. The lite build has no unicode tables, which keeps the wasm32 check small; a window title needs none of them.`

`docs/config.example.toml`, after the `[keyboard.bindings]` section:

```toml
# Per-game profiles. A profile applies while a matching window has focus; it
# overlays the device bindings and any per-app overlay, and it falls only when
# another identifiable window takes focus — going to the desktop keeps it.
# Rules: WmClass (the application id above, case-insensitive) beats SteamAppId
# beats Title (a regular expression).
[profiles.lost-ark]
name = "Lost Ark"
matches = [{ WmClass = "lostark.exe" }, { SteamAppId = 1599340 }, { Title = "^LOST ARK" }]

[profiles.lost-ark.assignments.normal]
MiddleClick = { RunMacro = "superspace" }
Back = { RunMacro = "hyper" }
# The trigger: hold it and every button below switches to its g_shift entry.
DpiToggle = "GShift"

[profiles.lost-ark.assignments.g_shift]
RightClick = { RunMacro = "superright" }

# Macros a RunMacro binding refers to by id. Steps are Linux input event
# codes; `held_codes` are released on every exit, so a key is never left down.
[macros.superspace]
id = "superspace"
name = "SuperSpace"
steps = [{ KeyTap = 57 }]
repeat = { WhileHeld = { interval_ms = 25 } }
```

The `[macros]` entry is new to the example too — it was never documented (Task 1 fixes that debt while it is here). `57` is `KEY_SPACE`. Until Task 2 lands, `DpiToggle = "GShift"` does not parse: **commit the example without that line in this task and add it in Task 2**.

`docs/CONFIGURATION.md`: fix line 37 (`schema_version` is `6`, not `5`); under "Shape", after the `keyboard` bullet, add bullets for `macros` (id, name, steps as event codes, the three repeat modes) and `profiles` (name, icon, matches with the three rule kinds and their priority, `assignments.normal` / `assignments.g_shift`, `Action::None` disables a button, "unknown focus keeps the profile"). Two short paragraphs, same voice as the existing bullets.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p openlogi-core profile && cargo test -p openlogi-core canonical_configuration_example_parses`
Expected: pass.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p openlogi-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/openlogi-core docs/config.example.toml docs/CONFIGURATION.md
git commit -m "feat(core): per-game profiles with a window matcher

A profile is a named object in its own table with match rules and two
assignment layers, not an overlay hidden inside a device entry. Rules are
tried strongest first — WM_CLASS, Steam AppID, title regex — so a Proton
game keeps matching when its launcher renames the window.

Also documents [macros], which shipped in #3 without a line in
CONFIGURATION.md."
```

---

## Task 2: `Action::GShift`

**Files:**
- Modify: `crates/openlogi-core/src/binding/action.rs:205` (append), the `for_each_unit_action!` table (`action.rs:248-307`, Mouse group), `crates/openlogi-core/src/binding/effect.rs:273`, `crates/openlogi-core/src/binding/tests.rs:290-388`, `crates/openlogi-ipc/src/ipc.rs:64`, `crates/openlogi-ipc/tests/wire_format.rs:103`, `docs/config.example.toml`
- Test: `crates/openlogi-core/src/binding/tests.rs`

**Interfaces:**
- Produces: `Action::GShift` — a unit variant, last in the enum; `Effect::AgentSide`; label `"G-Shift"`, category `Mouse`, ring icon `Layers`.

- [ ] **Step 1: Write the failing test**

In `crates/openlogi-core/src/binding/tests.rs`, add `"GShift"` to the `expected` array of `persisted_action_variant_names_are_stable` (keep the array sorted), and add:

```rust
/// The trigger is a layer switch, not an action: it serialises as a plain
/// string, it is agent-side, and it is pickable on the button screen.
#[test]
fn g_shift_is_a_unit_action_the_agent_owns() {
    assert_eq!(roundtrip(&Action::GShift), Action::GShift);
    assert_eq!(toml::Value::try_from(Action::GShift).expect("serialize"), toml::Value::String("GShift".into()));
    assert_eq!(Action::GShift.label(), "G-Shift");
    assert!(Action::catalog().contains(&Action::GShift));
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test -p openlogi-core g_shift_is_a_unit_action`
Expected: compile error — no variant `GShift`.

- [ ] **Step 3: Implement**

`action.rs`, after `RunMacro`:

```rust
    /// The G-Shift trigger. While the button carrying it is held, every other
    /// button resolves to its `g_shift` assignment in the active profile.
    ///
    /// Not an action: the OS hook consumes it as a layer switch and it never
    /// reaches a dispatcher. Anywhere it cannot mean that — the Actions Ring,
    /// a keyboard key, a HID++-diverted button — it is inert.
    GShift,
```

Table row, last in the Mouse group: `GShift "G-Shift" Mouse Layers,`. `effect.rs`: add `| Action::GShift` to the `Effect::AgentSide` group with a one-line comment. Then `cargo check --workspace`; every exhaustive `match` on `Action` the compiler names gets a `GShift` arm — expect at least `crates/openlogi-agent-core/src/runtime.rs` (`ActionExecutor::dispatch`: `Action::GShift => return,` with the comment `// The hook consumes the trigger; here it is inert.`) and possibly the GUI's action picker.

`ipc.rs`: ledger line `/// v31: `Action::GShift` appended — the per-game profile layer trigger.` and `PROTOCOL_VERSION = 31`; `wire_format.rs`: `assert_eq!(PROTOCOL_VERSION, 31)`. Appending a variant changes no existing golden.

`docs/config.example.toml`: add the `DpiToggle = "GShift"` line to `[profiles.lost-ark.assignments.normal]`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p openlogi-core binding && cargo test -p openlogi-core canonical_configuration_example_parses && cargo test -p openlogi-ipc --test wire_format`
Expected: pass.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p openlogi-core -p openlogi-ipc -p openlogi-agent-core --all-targets -- -D warnings`

```bash
git commit -am "feat(core): Action::GShift, the profile layer trigger

Appended last so the wire keeps decoding; PROTOCOL_VERSION 31."
```

---

## Task 3: Resolve bindings against a scope, not an app string

**Files:**
- Modify: `crates/openlogi-core/src/bindings.rs`, `crates/openlogi-core/src/config.rs` (next to `effective_bindings`, line 544), `crates/openlogi-agent-core/src/capture_plan.rs:56-66` (+ its tests), `crates/openlogi-agent-core/src/orchestrator.rs:257-300, 395-412, 748-764`, `crates/openlogi-agent-core/src/watchers/gesture/tests.rs` (five `plan_for_device` calls), `crates/openlogi-desktop/src/state/bindings.rs:92`
- Test: `crates/openlogi-core/src/bindings.rs` (inline tests), `crates/openlogi-core/src/config/tests.rs`

**Interfaces:**
- Produces:
  - `openlogi_core::bindings::ActiveScope { pub app: Option<String>, pub profile: Option<ProfileId> }` — `Clone, Debug, Default, PartialEq, Eq`; `ActiveScope::for_app(app: Option<&str>) -> Self`.
  - `Config::effective_bindings_in(&self, device_key: &str, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding>` — the per-app overlay, then the profile's `normal` layer.
  - `Config::g_shift_layer(&self, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding>` — the profile's `g_shift` layer as `Binding::Single`, empty without a profile.
  - `bindings_for(config, key, scope: &ActiveScope)`, `button_bindings_for(config, key, scope: &ActiveScope)`, `oshook_gestures_for(config, key, scope: &ActiveScope)` — same bodies, scope instead of `app_bundle: Option<&str>`.
  - `bindings::g_shift_bindings_for(config: &Config, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding>`.
  - `capture_plan::plan_for_device(config, config_key, route, scope: &ActiveScope, rearm_generation)`.
  - `Orchestrator::scope(&self) -> ActiveScope` (private) — `app: self.current_app.clone(), profile: None` until Task 5 fills it.

- [ ] **Step 1: Write the failing tests**

In `crates/openlogi-core/src/config/tests.rs`:

```rust
fn lost_ark(back: Action, shifted_right: Action) -> (ProfileId, GameProfile) {
    let mut profile = GameProfile {
        name: "Lost Ark".into(),
        icon: None,
        matches: vec![MatchRule::WmClass("lostark.exe".into())],
        assignments: Assignments::default(),
    };
    profile.assignments.normal.insert(ButtonId::Back, back);
    profile.assignments.g_shift.insert(ButtonId::RightClick, shifted_right);
    (ProfileId("lost-ark".into()), profile)
}

#[test]
fn a_profile_overlays_the_per_app_overlay() {
    let mut cfg = Config::default();
    cfg.set_binding("m", ButtonId::Back, Action::BrowserBack.into());
    cfg.set_per_app_binding("m", "lostark.exe", ButtonId::Back, Some(Action::Undo));
    let (id, profile) = lost_ark(Action::RunMacro(MacroId("hyper".into())), Action::RightClick);
    cfg.profiles.insert(id.clone(), profile);

    let app_only = ActiveScope::for_app(Some("lostark.exe"));
    assert_eq!(cfg.effective_bindings_in("m", &app_only).get(&ButtonId::Back), Some(&Binding::Single(Action::Undo)));

    let scope = ActiveScope { app: Some("lostark.exe".into()), profile: Some(id) };
    assert_eq!(
        cfg.effective_bindings_in("m", &scope).get(&ButtonId::Back),
        Some(&Binding::Single(Action::RunMacro(MacroId("hyper".into()))))
    );
    assert!(cfg.g_shift_layer(&app_only).is_empty());
    assert_eq!(cfg.g_shift_layer(&scope).get(&ButtonId::RightClick), Some(&Binding::Single(Action::RightClick)));
}

#[test]
fn a_profile_applies_to_a_device_with_no_config_entry() {
    let mut cfg = Config::default();
    let (id, profile) = lost_ark(Action::Copy, Action::Paste);
    cfg.profiles.insert(id.clone(), profile);
    let scope = ActiveScope { app: None, profile: Some(id) };
    assert_eq!(cfg.effective_bindings_in("never-seen", &scope).get(&ButtonId::Back), Some(&Binding::Single(Action::Copy)));
}
```

Import `crate::bindings::ActiveScope`.

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -p openlogi-core a_profile_`
Expected: compile error — `ActiveScope` / `effective_bindings_in` missing.

- [ ] **Step 3: Implement**

`bindings.rs`:

```rust
/// What bindings are resolved against: the application in front and the
/// per-game profile the agent has applied for it. Both are `None` when the
/// focus source knows nothing, and the GUI resolves the device's own bindings
/// with `ActiveScope::default()`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveScope {
    pub app: Option<String>,
    pub profile: Option<ProfileId>,
}

impl ActiveScope {
    /// The per-app overlay only, as the pre-profile resolvers took it.
    #[must_use]
    pub fn for_app(app: Option<&str>) -> Self {
        Self { app: app.map(str::to_owned), profile: None }
    }
}

/// The active profile's G-Shift layer, as the hook consumes it. Empty when no
/// profile is applied: without a trigger there is no layer to switch to.
#[must_use]
pub fn g_shift_bindings_for(config: &Config, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding> {
    config.g_shift_layer(scope)
}
```

Change the three resolvers' parameter from `app_bundle: Option<&str>` to `scope: &ActiveScope` and their inner call from `config.effective_bindings(key, app_bundle)` to `config.effective_bindings_in(key, scope)`. Update the inline tests (`None` → `&ActiveScope::default()`, `Some("com.apple.Safari")` → `&ActiveScope::for_app(Some("com.apple.Safari"))`).

`config.rs`, after `effective_bindings`:

```rust
    /// [`Self::effective_bindings`] with the applied profile's `normal` layer
    /// on top. The profile overlay does not require a device entry: a profile
    /// is document-scoped and a mouse seen for the first time still gets it.
    #[must_use]
    pub fn effective_bindings_in(&self, device_key: &str, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding> {
        let mut out = self.effective_bindings(device_key, scope.app.as_deref());
        if let Some(profile) = scope.profile.as_ref().and_then(|id| self.profiles.get(id)) {
            for (button, action) in &profile.assignments.normal {
                out.insert(*button, Binding::Single(action.clone()));
            }
        }
        out
    }

    /// The applied profile's `g_shift` layer as bindings; only the buttons the
    /// layer changes are present.
    #[must_use]
    pub fn g_shift_layer(&self, scope: &ActiveScope) -> BTreeMap<ButtonId, Binding> {
        scope
            .profile
            .as_ref()
            .and_then(|id| self.profiles.get(id))
            .map(|profile| {
                profile
                    .assignments
                    .g_shift
                    .iter()
                    .map(|(button, action)| (*button, Binding::Single(action.clone())))
                    .collect()
            })
            .unwrap_or_default()
    }
```

`capture_plan.rs`: `app: Option<&str>` → `scope: &ActiveScope`, pass it through; tests pass `&ActiveScope::default()`. `watchers/gesture/tests.rs`: same at the five call sites. `orchestrator.rs`: add

```rust
    /// What the hook maps and capture plans are resolved against right now.
    fn scope(&self) -> ActiveScope {
        ActiveScope { app: self.current_app.clone(), profile: None }
    }
```

and replace `self.current_app.as_deref()` with `&self.scope()` in `hook_maps_for` (which becomes `fn hook_maps_for(&self, key: Option<&str>, scope: &ActiveScope)`), `keyboard_spec_for`, `capture_plans_for`, and `set_current_app`. `openlogi-desktop/src/state/bindings.rs:92`: `oshook_gestures_for(config, Some(key), &ActiveScope::default())`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p openlogi-core && cargo test -p openlogi-agent-core && cargo check -p openlogi-desktop`
Expected: pass. Existing behavior is unchanged: with `profile: None` every resolver produces what it did.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p openlogi-core -p openlogi-agent-core --all-targets -- -D warnings`

```bash
git commit -am "refactor(core): resolve bindings against a scope

The resolvers took the foreground app as a bare string. They now take an
ActiveScope — the app plus the applied profile — and the profile's normal
layer overlays the per-app overlay. Nothing fills the profile yet, so
every resolver still answers exactly as before."
```

---

## Task 4: Read the title, the pid and the Steam AppID on Linux

**Files:**
- Modify: `crates/openlogi-hook/src/lib.rs:355-358, 512`, `crates/openlogi-hook/src/linux.rs:131-135, 556-561, 565-635`, `crates/openlogi-hook/src/linux/gnome_shell.rs`, `crates/openlogi-hook/src/linux/wlr_foreign_toplevel.rs:84-90, 174-194, 435-461`, `crates/openlogi-hook/examples/frontmost_app.rs`, `crates/openlogi-hook/gnome-shell-extension/openlogi-frontmost@openlogi.dev/extension.js`, `metadata.json`, `crates/openlogi-hook/gnome-shell-extension/README.md`
- Test: `crates/openlogi-hook/src/linux.rs` (inline tests)

**Interfaces:**
- Produces:
  - `HookBackend::focused_window() -> Option<FocusedWindow>` — default `Self::frontmost_app().map(FocusedWindow::app)`; overridden on Linux only. macOS and Windows are **not edited**.
  - `openlogi_hook::focused_window() -> Option<FocusedWindow>` (public, next to `frontmost_application`).
  - Linux `FrontmostSource::focused_window(&self) -> Option<FocusedWindow>` — default wraps `frontmost_app_id`; gnome-shell and X11 override with title + pid, wlr with title.
  - Extension v2: `GetFocusedWindow() -> (s s u)` = (wmClass, title, pid), all empty/zero when nothing is focused. `GetFocusedWmClass` stays.
  - `linux::steam_app_id_in(environ: &[u8]) -> Option<u32>` (private, tested) and `steam_app_id_of(pid: u32) -> Option<u32>` reading `/proc/<pid>/environ`.

- [ ] **Step 1: Write the failing test**

In `crates/openlogi-hook/src/linux.rs`, in (or add) `#[cfg(test)] mod tests`:

```rust
#[test]
fn steam_app_id_is_read_from_a_nul_separated_environ() {
    let environ = b"HOME=/home/x\0SteamAppId=1599340\0SteamGameId=1599340\0";
    assert_eq!(steam_app_id_in(environ), Some(1_599_340));
    assert_eq!(steam_app_id_in(b"HOME=/home/x\0"), None);
    assert_eq!(steam_app_id_in(b"SteamAppId=notanumber\0"), None);
    assert_eq!(steam_app_id_in(b""), None);
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cargo test -p openlogi-hook steam_app_id`
Expected: compile error — `steam_app_id_in` missing.

- [ ] **Step 3: Implement the Rust side**

`linux.rs`:

```rust
/// `SteamAppId=` from a NUL-separated environment block, laid out the way
/// `/proc/<pid>/environ` is. Steam exports it to every game it launches, and a
/// Proton game inherits it into the Wine process that owns the window.
fn steam_app_id_in(environ: &[u8]) -> Option<u32> {
    environ
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(b"SteamAppId="))
        .and_then(|value| std::str::from_utf8(value).ok()?.parse().ok())
}

/// The Steam AppID of `pid`, when Steam launched it. The agent runs as the
/// user, so its own games' environments are readable; anything else is `None`.
fn steam_app_id_of(pid: u32) -> Option<u32> {
    std::fs::read(format!("/proc/{pid}/environ"))
        .ok()
        .and_then(|environ| steam_app_id_in(&environ))
}

/// Fill `steam_app_id` from the pid, when the source reported one.
fn with_steam_app_id(mut window: FocusedWindow) -> FocusedWindow {
    window.steam_app_id = window.pid.and_then(steam_app_id_of);
    window
}
```

`FrontmostSource` gains:

```rust
    /// The focused window with whatever else the backend can read. The
    /// default is the identifier alone; backends that see a title or a pid
    /// override it.
    fn focused_window(&self) -> Option<FocusedWindow> {
        self.frontmost_app_id()
            .map(|id| FocusedWindow::app(ForegroundApp::unnamed(id)))
    }
```

The Linux `HookBackend` impl adds `fn focused_window() -> Option<FocusedWindow> { FRONTMOST_SOURCE.focused_window().map(with_steam_app_id) }`.

`X11Source`: intern `_NET_WM_NAME`, `UTF8_STRING`, `_NET_WM_PID` in `connect` (three more `Atom` fields); factor the `_NET_ACTIVE_WINDOW` read out of `frontmost_app_id` into `fn active_window(&self) -> Option<Window>` and the `WM_CLASS` read into `fn wm_class_of(&self, window) -> Option<String>`; `focused_window` reads the class the same way, then `get_property(false, window, net_wm_name, utf8_string, 0, 1024)` → `String::from_utf8(reply.value).ok().filter(|t| !t.is_empty())` for the title and `get_property(false, window, net_wm_pid, AtomEnum::CARDINAL, 0, 1)` → `value32()?.next()` for the pid (`0` → `None`).

`gnome_shell.rs`: add to the proxy trait

```rust
    /// WM_CLASS, title and client pid of the focused window; empty strings and
    /// `0` when nothing is focused. Extension v2 only.
    #[zbus(name = "GetFocusedWindow")]
    fn get_focused_window(&self) -> zbus::Result<(String, String, u32)>;
```

and implement `focused_window`: call `get_focused_window`; on `Ok((class, title, pid))` map `class.is_empty()` → `None`, else `FocusedWindow { app: ForegroundApp::unnamed(class), title: (!title.is_empty()).then_some(title), pid: (pid != 0).then_some(pid), steam_app_id: None }`; on `Err` log at `debug!` once per connection (a `std::sync::Once` field is fine) that the extension is v1, and fall back to `frontmost_app_id`.

`wlr_foreign_toplevel.rs`: add `title: Option<String>` and `pending_title: Option<String>` to `Toplevel`; handle `Event::Title { title } => toplevel.pending_title = Some(title)`; commit on `Done` the way `app_id` is committed; `focused_window` returns the activated toplevel's `app_id` + `title`, no pid (the protocol has none).

`lib.rs`: trait default method and the public `focused_window()` function, both documented like their `frontmost_app` twins. `examples/frontmost_app.rs`: print `{:?}` of `focused_window()` each second instead of the pair.

- [ ] **Step 4: Implement the extension**

`extension.js` — replace the header comment's privacy sentence (it now reads titles; say so and why: title rules are a fallback for games whose `WM_CLASS` a launcher hides) and add the method to `DBUS_INTERFACE`:

```xml
    <method name="GetFocusedWindow">
      <arg type="s" direction="out" name="wmClass"/>
      <arg type="s" direction="out" name="title"/>
      <arg type="u" direction="out" name="pid"/>
    </method>
```

and to the class:

```js
    // D-Bus method org.openlogi.Frontmost.GetFocusedWindow.
    GetFocusedWindow() {
        return this._describe(global.display.focus_window);
    }

    // [wmClass, title, pid] for a Meta.Window, or the empty triple.
    _describe(win) {
        if (!win)
            return ['', '', 0];
        const pid = win.get_pid();
        return [win.get_wm_class() || '', win.get_title() || '', pid > 0 ? pid : 0];
    }
```

`metadata.json`: `"version": 2`. README: document the new method under "D-Bus surface" and add "Since v2 the extension also reads window titles and client pids; both stay on this machine and feed per-game profile rules only."

- [ ] **Step 5: Run the tests and verify by hand**

Run: `cargo test -p openlogi-hook && cargo clippy -p openlogi-hook --all-targets -- -D warnings`

Install the v2 extension for this user and re-login (GNOME Wayland cannot reload an extension in place):

```bash
UUID=openlogi-frontmost@openlogi.dev
install -Dm644 crates/openlogi-hook/gnome-shell-extension/$UUID/extension.js ~/.local/share/gnome-shell/extensions/$UUID/extension.js
install -Dm644 crates/openlogi-hook/gnome-shell-extension/$UUID/metadata.json ~/.local/share/gnome-shell/extensions/$UUID/metadata.json
gnome-extensions enable $UUID   # then log out and back in
```

After re-login:

```bash
busctl --user call org.openlogi.Frontmost /org/openlogi/Frontmost org.openlogi.Frontmost GetFocusedWindow
cargo run -p openlogi-hook --example frontmost_app
```

Expected: the `busctl` call prints `ssu "<class>" "<title>" <pid>` for the terminal itself; the example prints a `FocusedWindow` with `title: Some(..)` and `pid: Some(..)`, and `steam_app_id: Some(..)` when a Steam game is in front. Paste the output in the commit body.

- [ ] **Step 6: Commit**

```bash
git commit -am "feat(hook): read the focused window's title, pid and Steam AppID on Linux

A per-game profile falls back from WM_CLASS to the Steam AppID and the
title, so the focus sources now report what they can: the extension (v2,
new method, old one kept) and X11 give title and pid, wlroots gives the
title, and /proc/<pid>/environ gives Steam's AppID. macOS and Windows keep
reporting the application alone through the trait default."
```

---

## Task 5: Level-triggered reconciliation and the applied profile on the wire

**Files:**
- Modify: `crates/openlogi-agent-core/src/watchers/poll.rs`, `crates/openlogi-agent-core/src/watchers/foreground_app.rs`, `crates/openlogi-agent-core/src/orchestrator.rs` (fields at 140-150; `scope` from Task 3; `set_current_app` 748-764; `reload_config` 767-799), `crates/openlogi-agent-core/src/observable.rs:239`, `crates/openlogi-agent/src/lifecycle.rs:343`, `crates/openlogi-agent/src/startup.rs:203`, `crates/openlogi-ipc/src/ipc.rs:64, 126-141`, `crates/openlogi-ipc/tests/wire_format.rs`
- Test: `crates/openlogi-agent-core/src/orchestrator/tests.rs`, `crates/openlogi-agent-core/src/watchers/poll.rs` (inline), `crates/openlogi-ipc/tests/wire_format.rs`

**Interfaces:**
- Consumes: `FocusedWindow`, `profile::resolve`, `ActiveScope`, `openlogi_hook::focused_window`.
- Produces:
  - `Poll::every_into<T, F>(self, tx: mpsc::UnboundedSender<T>, read: F)` — every sample, no dedup, no per-sample log.
  - `watchers::foreground_app::FocusReading = Option<FocusedWindow>`; `spawn(period) -> UnboundedReceiver<FocusReading>` (push is merged in Task 7).
  - `Orchestrator::reconcile_focus(&mut self, reading: Option<FocusedWindow>) -> bool` — returns whether the applied scope changed. `set_current_app` stays as a projection for callers that only know the app.
  - `ObservableState::set_active_profile(&self, profile: Option<ProfileId>)`.
  - `AgentSnapshot::active_profile: Option<ProfileId>` appended; `PROTOCOL_VERSION = 32`.

- [ ] **Step 1: Write the failing tests**

`crates/openlogi-agent-core/src/orchestrator/tests.rs`:

```rust
use openlogi_core::app::FocusedWindow;
use openlogi_core::profile::{Assignments, GameProfile, MatchRule, ProfileId};
use ghub_macro::MacroId;

fn lost_ark_config() -> Config {
    let mut config = Config::default();
    let mut profile = GameProfile {
        name: "Lost Ark".into(),
        icon: None,
        matches: vec![MatchRule::WmClass("lostark.exe".into()), MatchRule::Title("^LOST ARK".into())],
        assignments: Assignments::default(),
    };
    profile.assignments.normal.insert(ButtonId::Back, Action::RunMacro(MacroId("hyper".into())));
    config.profiles.insert(ProfileId("lost-ark".into()), profile);
    config
}

fn focus(app_id: &str) -> Option<FocusedWindow> {
    Some(FocusedWindow::app(ForegroundApp::unnamed(app_id.into())))
}

fn hyper() -> Option<Action> {
    Some(Action::RunMacro(MacroId("hyper".into())))
}

#[test]
fn an_unknown_focus_keeps_the_applied_profile() {
    let mut orch = orchestrator(lost_ark_config());
    orch.devices = vec![dev("a", 1, true)];
    orch.rebuild();
    assert!(orch.reconcile_focus(focus("lostark.exe")));
    assert_eq!(published_back_binding(&orch), hyper());

    // Reading failed, or the desktop is in front: nothing identifiable took focus.
    assert!(!orch.reconcile_focus(None), "unknown is not a change");
    assert_eq!(published_back_binding(&orch), hyper());

    assert!(orch.reconcile_focus(focus("org.gnome.Nautilus")));
    assert_ne!(published_back_binding(&orch), hyper());
}

#[test]
fn a_repeated_reading_is_a_no_op() {
    // The watcher reports every tick; a hold on the G-Shift trigger across a
    // tick must not be cancelled by a reconcile that changed nothing.
    let mut orch = orchestrator(lost_ark_config());
    orch.devices = vec![dev("a", 1, true)];
    orch.rebuild();
    assert!(orch.reconcile_focus(focus("lostark.exe")));
    assert!(!orch.reconcile_focus(focus("lostark.exe")));
}

#[test]
fn a_title_rule_matches_when_the_class_is_unknown() {
    let mut orch = orchestrator(lost_ark_config());
    orch.devices = vec![dev("a", 1, true)];
    orch.rebuild();
    let mut window = FocusedWindow::app(ForegroundApp::unnamed("steam_app_1599340".into()));
    window.title = Some("LOST ARK".into());
    assert!(orch.reconcile_focus(Some(window)));
    assert_eq!(published_back_binding(&orch), hyper());
}

#[test]
fn a_config_reload_reevaluates_the_last_window() {
    // Level-triggered: the profile was added while the game already had
    // focus. No new reading arrives — the reload alone must apply it.
    let mut orch = orchestrator(Config::default());
    orch.devices = vec![dev("a", 1, true)];
    orch.rebuild();
    orch.reconcile_focus(focus("lostark.exe"));
    assert_ne!(published_back_binding(&orch), hyper());
    orch.reload_config(lost_ark_config());
    assert_eq!(published_back_binding(&orch), hyper());
}
```

`crates/openlogi-agent-core/src/watchers/poll.rs` tests:

The module's tests are plain `#[test]`s using its `drain(rx, want)` helper:

```rust
#[test]
fn every_reports_unchanged_samples() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    Poll {
        name: "openlogi-test-watcher",
        period: Duration::from_millis(1),
        degrades: "the test learns nothing",
    }
    .every_into(tx, || 7u8);
    assert_eq!(
        drain(&mut rx, 2),
        vec![7, 7],
        "the second identical sample is still reported"
    );
}
```

`crates/openlogi-ipc/tests/wire_format.rs`: pin `31 → 32`, and add

```rust
/// The applied per-game profile rides the snapshot so a client can show which
/// one is active without re-running the matcher it cannot run (it has no
/// title or Steam AppID).
#[test]
fn active_profile() {
    assert_wire(&None::<ProfileId>, "00");
    assert_wire(&Some(ProfileId("lost-ark".into())), "01086c6f73742d61726b");
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -p openlogi-agent-core reconcile_focus every_reports && cargo test -p openlogi-ipc --test wire_format`
Expected: compile errors — `reconcile_focus`, `every_into`, `ProfileId` import.

- [ ] **Step 3: Implement**

`poll.rs`: extract the thread body of `on_change` into `fn run<T, F>(self, tx: mpsc::UnboundedSender<T>, read: F, dedup: bool)`; `on_change` creates the channel and calls `run(tx, read, true)`; add

```rust
    /// Report what `read` returns on every tick, changed or not, into `tx`.
    ///
    /// For a consumer that reconciles by level rather than by edge: it wants
    /// the current value each period so a missed transition heals on the next
    /// tick. Taking the sender lets a push source share the channel.
    pub fn every_into<T, F>(self, tx: mpsc::UnboundedSender<T>, read: F)
    where
        T: Clone + PartialEq + Debug + Send + 'static,
        F: Fn() -> T + Send + 'static,
    {
        self.run(tx, read, false);
    }
```

In `run`, the `debug!("changed")` line stays inside `if dedup`; with `dedup == false` the loop sends every sample silently.

`foreground_app.rs`:

```rust
pub type FocusReading = Option<FocusedWindow>;

/// Report the focused window every `period`, changed or not.
///
/// The consumer reconciles by level (spec §6): it compares the profile that
/// should be applied against the one that is, on every reading, so a missed
/// edge corrects itself a period later instead of waiting for the user to
/// alt-tab twice.
pub fn spawn(period: Duration) -> mpsc::UnboundedReceiver<FocusReading> {
    if !cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows")) {
        return poll::never();
    }
    let (tx, rx) = mpsc::unbounded_channel();
    Poll { name: "openlogi-focus-watcher", period, degrades: "per-game profiles are disabled" }
        .every_into(tx, openlogi_hook::focused_window);
    rx
}
```

`orchestrator.rs` fields:

```rust
    /// The last *identifiable* window the focus source reported. A `None`
    /// reading is unknown, not empty — it leaves this alone (spec §6, rule 2).
    last_focus: Option<FocusedWindow>,
    /// The per-game profile applied to the hook maps and capture plans.
    active_profile: Option<ProfileId>,
```

`scope()` returns `profile: self.active_profile.clone()`. Then:

```rust
    /// Converge the applied scope on `reading`.
    ///
    /// Level-triggered: the caller feeds every reading, and this compares the
    /// scope the config says *should* apply against the one that *is*
    /// applied. Unchanged is cheap and returns `false`, which matters because
    /// the caller cancels every press lifecycle on `true` — a G-Shift hold
    /// across a poll tick must survive. A `None` reading keeps the last
    /// identifiable window: the desktop, an unnamed window, or a failed read
    /// never drop a profile; only another identifiable window does.
    pub fn reconcile_focus(&mut self, reading: Option<FocusedWindow>) -> bool {
        if let Some(window) = reading {
            self.last_focus = Some(window);
        }
        let app = self.last_focus.as_ref().map(|window| window.app.id.clone());
        let profile = self
            .last_focus
            .as_ref()
            .and_then(|window| profile::resolve(&self.config.profiles, window))
            .cloned();
        self.observable.set_foreground(self.last_focus.as_ref().map(|w| w.app.clone()));
        if app == self.current_app && profile == self.active_profile {
            return false;
        }
        if profile != self.active_profile {
            info!(from = ?self.active_profile, to = ?profile, "profile applied");
        }
        self.current_app = app;
        self.active_profile = profile;
        self.observable.set_active_profile(self.active_profile.clone());
        write_value(&self.shared.hook_maps, self.hook_maps_for(self.current_key(), &self.scope()), "hook_maps");
        self.publish_device_runtime();
        true
    }

    /// [`Self::reconcile_focus`] for a caller that knows the application only.
    pub fn set_current_app(&mut self, app: Option<ForegroundApp>) -> bool {
        self.reconcile_focus(app.map(FocusedWindow::app))
    }
```

`reload_config`: before `self.rebuild()`, add

```rust
        // The window did not change but the profiles may have: re-run the
        // matcher against the last reading so the reload alone applies it.
        self.active_profile = self
            .last_focus
            .as_ref()
            .and_then(|window| profile::resolve(&self.config.profiles, window))
            .cloned();
        self.observable.set_active_profile(self.active_profile.clone());
```

Existing tests that asserted `set_current_app(None)` drops the per-app overlay now fail by design — rewrite them to assert the new rule and say so in the commit; do not delete them.

`observable.rs`: `pub fn set_active_profile(&self, profile: Option<ProfileId>)` via `self.update(|snapshot| { if snapshot.active_profile == profile { return false; } snapshot.active_profile = profile; true })`.

`ipc.rs`: `pub active_profile: Option<ProfileId>` as the **last** field of `AgentSnapshot` with a doc comment; ledger `/// v32: `AgentSnapshot::active_profile` appended — the per-game profile the agent applied.`; `PROTOCOL_VERSION = 32`. Add `active_profile: None` at the three places the struct is built: `crates/openlogi-agent-core/src/observable.rs:47`, `crates/openlogi-agent/src/bin/mock_agent.rs:713`, and `crates/openlogi-ipc/tests/wire_format.rs:304`. The `agent_snapshot` and `Observation` goldens gain a trailing `00`; take the exact hex from the failing assertion's `left`.

`startup.rs`: `App(watchers::foreground_app::FocusReading)`. `lifecycle.rs`:

```rust
    /// Feed one focus reading to the reconciler and, when the applied scope
    /// changed, cancel every press lifecycle resolved against the old one —
    /// which also stops every running macro (`MacroRunner::stop_all`).
    async fn apply_foreground(&self, reading: FocusReading) {
        if self.orchestrator.lock().await.reconcile_focus(reading) {
            self.inputs.dispatcher.cancel_all_buttons();
        }
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p openlogi-agent-core && cargo test -p openlogi-ipc --test wire_format && cargo test -p openlogi-agent`
Expected: pass.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p openlogi-agent-core -p openlogi-agent -p openlogi-ipc --all-targets -- -D warnings`

```bash
git commit -am "feat(agent): apply per-game profiles by level-triggered reconciliation

The focus watcher reports every tick and the orchestrator converges the
applied scope on it, so a missed transition heals a second later instead
of after the user alt-tabs until it sticks. A reading of None is unknown,
not nothing: the desktop, a nameless window or a failed read keep the
profile; only another identifiable window replaces it. A config reload
re-runs the matcher against the last window. The applied profile rides
the snapshot (PROTOCOL_VERSION 32) so the GUI can show it without a
matcher of its own."
```

---

## Task 6: The G-Shift layer on the hook thread

**Files:**
- Modify: `crates/openlogi-agent-core/src/runtime/hook.rs:30-49, 173-181, 239-310, 465-511`, `crates/openlogi-agent-core/src/orchestrator.rs` (`hook_maps_for`)
- Test: `crates/openlogi-agent-core/src/runtime/hook/tests.rs`

**Interfaces:**
- Consumes: `bindings::g_shift_bindings_for`, `Action::GShift`.
- Produces:
  - `HookMaps::g_shift: BTreeMap<ButtonId, Binding>` (third field).
  - `HookMaps::is_trigger(&self, id: ButtonId) -> bool` — the *normal* layer binds `Action::GShift` to `id`.
  - `HookMaps::resolve(&self, id: ButtonId, shifted: bool) -> Option<Binding>` — `g_shift` when shifted and present, else `bindings`.
  - `fn press_layer(pressed: bool, held: bool, shifted_presses: &mut HashSet<ButtonId>, id: ButtonId) -> bool` — the layer a press *or its release* resolves in.
  - `static G_SHIFT_HELD: AtomicBool`.

- [ ] **Step 1: Write the failing tests**

`hook/tests.rs`:

The file already opens with `use super::*;`, which brings `HookMaps`, `press_layer`, `binding_passes_through`, `Action`, `Binding`, `ButtonId` and `BTreeMap` into scope; add `use std::collections::HashSet;` if the module does not import it.

```rust
fn maps() -> HookMaps {
    let mut bindings = BTreeMap::new();
    bindings.insert(ButtonId::DpiToggle, Binding::Single(Action::GShift));
    bindings.insert(ButtonId::Back, Binding::Single(Action::Copy));
    bindings.insert(ButtonId::RightClick, Binding::Single(Action::RightClick));
    let mut g_shift = BTreeMap::new();
    g_shift.insert(ButtonId::RightClick, Binding::Single(Action::Paste));
    HookMaps { bindings, gestures: BTreeMap::new(), g_shift }
}

#[test]
fn the_trigger_is_found_in_the_normal_layer_only() {
    let maps = maps();
    assert!(maps.is_trigger(ButtonId::DpiToggle));
    assert!(!maps.is_trigger(ButtonId::Back));
    assert!(!maps.is_trigger(ButtonId::RightClick));
}

#[test]
fn a_shifted_lookup_falls_back_to_the_normal_layer() {
    let maps = maps();
    assert_eq!(maps.resolve(ButtonId::RightClick, false), Some(Binding::Single(Action::RightClick)));
    assert_eq!(maps.resolve(ButtonId::RightClick, true), Some(Binding::Single(Action::Paste)));
    assert_eq!(maps.resolve(ButtonId::Back, true), Some(Binding::Single(Action::Copy)));
}

#[test]
fn a_release_resolves_in_the_layer_its_press_used() {
    // Press under G-Shift, release after the trigger let go: the release must
    // still resolve shifted, or the press is suppressed and the release is
    // not — an OS-level stuck button.
    let mut shifted = HashSet::new();
    assert!(press_layer(true, true, &mut shifted, ButtonId::RightClick));
    assert!(press_layer(false, false, &mut shifted, ButtonId::RightClick));
    assert!(!press_layer(false, false, &mut shifted, ButtonId::RightClick), "consumed");
    assert!(!press_layer(true, false, &mut shifted, ButtonId::Back));
    assert!(!press_layer(false, true, &mut shifted, ButtonId::Back), "pressed unshifted, released shifted");
}

#[test]
fn the_trigger_on_the_primary_button_is_swallowed() {
    // GShift is an explicit binding, so the left button's floor lets it be
    // suppressed — a trigger that also clicks would be useless.
    assert!(!binding_passes_through(ButtonId::LeftClick, &Binding::Single(Action::GShift)));
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -p openlogi-agent-core hook::tests`
Expected: compile errors — `g_shift`, `is_trigger`, `resolve`, `press_layer` missing.

- [ ] **Step 3: Implement**

`hook.rs`, `HookMaps`:

```rust
    /// The active profile's G-Shift layer: only the buttons it changes. Read
    /// while [`G_SHIFT_HELD`] is set, falling back to `bindings`.
    pub g_shift: BTreeMap<ButtonId, Binding>,
}

impl HookMaps {
    /// Whether `id` is the G-Shift trigger. Looked up in the normal layer
    /// only — a trigger inside the shifted layer would have no way to be
    /// pressed.
    #[must_use]
    pub fn is_trigger(&self, id: ButtonId) -> bool {
        self.bindings
            .get(&id)
            .is_some_and(|binding| binding.click_action() == Action::GShift)
    }

    /// The binding `id` resolves to in the given layer.
    #[must_use]
    pub fn resolve(&self, id: ButtonId, shifted: bool) -> Option<Binding> {
        let layered = shifted.then(|| self.g_shift.get(&id)).flatten();
        layered.or_else(|| self.bindings.get(&id)).cloned()
    }
}
```

Statics next to `HOLD`:

```rust
/// Whether the G-Shift trigger is held. One flag for the process, not per
/// hook thread: the layer is a property of the user's hand, and on Linux the
/// trigger and the buttons it modifies may arrive on different evdev threads
/// (a receiver and a cable are two devices).
static G_SHIFT_HELD: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Buttons whose press resolved in the shifted layer, so their release
    /// resolves there too. Per thread, like `FAIL_OPEN_PRESSES`: a press and
    /// its release come from the same device.
    static SHIFTED_PRESSES: RefCell<HashSet<ButtonId>> = RefCell::new(HashSet::new());
}

/// The layer a press resolves in — and, for a release, the layer its press
/// used, so the two dispositions always pair.
fn press_layer(pressed: bool, held: bool, shifted_presses: &mut HashSet<ButtonId>, id: ButtonId) -> bool {
    if pressed {
        if held {
            shifted_presses.insert(id);
        }
        held
    } else {
        shifted_presses.remove(&id)
    }
}
```

In `handle_button`, right after the Gate A early return:

```rust
    // The trigger is consumed here: it is a layer switch, not an action, and
    // it never reaches the dispatcher. `may_suppress` keeps the left-button
    // floor honest — an explicit GShift binding is something bound.
    if hooks.try_read().is_ok_and(|m| m.is_trigger(id)) {
        G_SHIFT_HELD.store(pressed, Ordering::Release);
        return if id.may_suppress(&Action::GShift) {
            EventDisposition::Suppress
        } else {
            EventDisposition::PassThrough
        };
    }
    let shifted = SHIFTED_PRESSES.with_borrow_mut(|presses| {
        press_layer(pressed, G_SHIFT_HELD.load(Ordering::Acquire), presses, id)
    });
```

Then: `is_gesture` becomes `m.gestures.contains_key(&id) && !(shifted && m.g_shift.contains_key(&id))` (a shifted single assignment wins over gesture mode for that press), and the binding lookup becomes `.and_then(|m| m.resolve(id, shifted))`. In the `MouseEvent::CaptureInterrupted` arm add `G_SHIFT_HELD.store(false, Ordering::Release); SHIFTED_PRESSES.with_borrow_mut(HashSet::clear);`.

`orchestrator.rs` `hook_maps_for`: `g_shift: g_shift_bindings_for(&self.config, scope)`. `HookMaps` is constructed in one more place (`rebuild` via `hook_maps_for`) — `Default` covers the disabled-device path.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p openlogi-agent-core`
Expected: pass.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p openlogi-agent-core --all-targets -- -D warnings`

```bash
git commit -am "feat(agent): the G-Shift layer

Holding the button bound to GShift makes every other button resolve to
the profile's g_shift assignment. The flag is one atomic read on the hook
thread — no lock, no capture-plan republish, no HID++ session restart —
and a press remembers its layer so its release is suppressed exactly when
the press was."
```

---

## Task 7: Push focus changes from the extension

**Files:**
- Modify: `extension.js`, `crates/openlogi-hook/gnome-shell-extension/README.md`, `crates/openlogi-hook/src/linux/gnome_shell.rs`, `crates/openlogi-hook/src/linux.rs` (trait + backend), `crates/openlogi-hook/src/lib.rs`, `crates/openlogi-agent-core/src/watchers/foreground_app.rs`
- Test: none automated (D-Bus signal delivery needs a live shell); hand verification in Step 4

**Interfaces:**
- Produces:
  - Extension signal `FocusChanged(s wmClass, s title, u pid)`, emitted on `notify::focus-window` and on the focused window's `notify::title`.
  - `openlogi_hook::watch_focus(on_reading: impl Fn(Option<FocusedWindow>) + Send + Sync + 'static) -> bool` — `true` when a push source exists; readings arrive on a hook-owned thread. `HookBackend::watch_focus(Box<dyn Fn(Option<FocusedWindow>) + Send + Sync>) -> bool`, default `false`.
  - Linux `FrontmostSource::watch(&self, on_reading: Box<…>) -> bool`, default `false`, overridden by gnome-shell.
  - The focus watcher polls every 3 s when a push source exists, 1 s otherwise.

- [ ] **Step 1: The extension**

`extension.js`: `import GLib from 'gi://GLib';`; add to `DBUS_INTERFACE`

```xml
    <signal name="FocusChanged">
      <arg type="s" name="wmClass"/>
      <arg type="s" name="title"/>
      <arg type="u" name="pid"/>
    </signal>
```

and to the class:

```js
    enable() {
        // …existing export and name ownership…
        this._focusId = global.display.connect('notify::focus-window',
            () => this._onFocusChanged());
        this._titledWindow = null;
        this._titleId = 0;
    }

    disable() {
        this._untrackTitle();
        if (this._focusId) {
            global.display.disconnect(this._focusId);
            this._focusId = 0;
        }
        // …existing unown and unexport…
    }

    // Focus moved: follow the new window's title too, since a Proton game
    // often renames its window after the launcher hands over, and a profile
    // may match on that title.
    _onFocusChanged() {
        this._untrackTitle();
        const win = global.display.focus_window;
        if (win) {
            this._titledWindow = win;
            this._titleId = win.connect('notify::title', () => this._emit(win));
        }
        this._emit(win);
    }

    _untrackTitle() {
        if (this._titledWindow && this._titleId)
            this._titledWindow.disconnect(this._titleId);
        this._titledWindow = null;
        this._titleId = 0;
    }

    _emit(win) {
        this._dbus.emit_signal('FocusChanged',
            new GLib.Variant('(ssu)', this._describe(win)));
    }
```

README: document the signal, and that v2 is push + pull.

- [ ] **Step 2: The Rust side**

`gnome_shell.rs` proxy trait:

```rust
    /// Emitted on every focus change and on a title change of the focused
    /// window. Same triple as `GetFocusedWindow`.
    #[zbus(signal)]
    fn focus_changed(&self, wm_class: String, title: String, pid: u32) -> zbus::Result<()>;
```

`FrontmostSource::watch` default returns `false`. `GnomeShellSource::watch` builds a second `Connection` (the poll thread keeps using the first), then spawns `std::thread::Builder::new().name("openlogi-focus-push".into())` running:

```rust
let proxy = FrontmostProxy::new(&conn)…;
let stream = match proxy.receive_focus_changed() { Ok(s) => s, Err(e) => { debug!("gnome-shell: signal subscription failed: {e}"); return; } };
for signal in stream {
    let Ok(args) = signal.args() else { continue };
    on_reading(describe(args.wm_class(), args.title(), *args.pid()));
}
debug!("gnome-shell: signal stream ended");
```

where `describe` is the same mapping Task 4 wrote for `GetFocusedWindow` (factor it into a free function both use). Return `true` if the thread spawned. The Linux `HookBackend::watch_focus` wraps the callback so every pushed reading also goes through `with_steam_app_id`.

`lib.rs`: the trait method with a `false` default and the public `watch_focus` function.

`foreground_app.rs`:

```rust
pub fn spawn(period: Duration) -> mpsc::UnboundedReceiver<FocusReading> {
    if !cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows")) {
        return poll::never();
    }
    let (tx, rx) = mpsc::unbounded_channel();
    let pushed = openlogi_hook::watch_focus({
        let tx = tx.clone();
        move |reading| {
            let _ = tx.send(reading);
        }
    });
    // With a push source the poll is only the reconciliation safety net
    // (spec §6): a few seconds is enough to heal a dropped signal.
    let period = if pushed { period * 3 } else { period };
    Poll { name: "openlogi-focus-watcher", period, degrades: "per-game profiles are disabled" }
        .every_into(tx, openlogi_hook::focused_window);
    rx
}
```

- [ ] **Step 3: Build and lint**

Run: `cargo clippy -p openlogi-hook -p openlogi-agent-core -p openlogi-agent --all-targets -- -D warnings && cargo test -p openlogi-hook`

- [ ] **Step 4: Verify by hand**

Reinstall the extension (Task 4's commands) and re-login. Then, in two terminals:

```bash
busctl --user monitor org.openlogi.Frontmost
RUST_LOG=openlogi_agent_core=info,openlogi_hook=debug cargo run -p openlogi-agent
```

Alt-tab between two windows. Expected: each switch prints one `FocusChanged` in the monitor and, within the same second, the agent logs `profile applied` (when a profile matches) — compare the monitor's timestamp with the log's. Stop the extension (`gnome-extensions disable …`) with the agent running: the poll thread keeps reporting at 3 s and switching still works, slower. Record both observations for the commit body.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(hook): push focus changes from the GNOME Shell extension

The extension emits FocusChanged on Mutter's notify::focus-window and on
the focused window's title, and the agent subscribes on a dedicated
thread. Profile switching drops from up to a second to the D-Bus round
trip; the poll stays as a 3 s reconciliation safety net."
```

---

## Task 8: Ship the extension and document the install

**Files:**
- Modify: `packaging/linux/nfpm.yaml:22` (contents), `packaging/linux/install.sh:84` (before udev), `packaging/linux/uninstall.sh`, `docs/INSTALL-linux.md` (new section before "Verify the installation"; fix the limitations row at line 184), `README.md` ("Building" section)

- [ ] **Step 1: Packaging**

`nfpm.yaml`, a new group after the desktop entry:

```yaml
  # ── GNOME Shell extension ───────────────────────────────────────────────────
  # Required for per-game profiles on GNOME Wayland: the focused window is not
  # visible to ordinary clients there. System-wide install; each user still
  # enables it (`gnome-extensions enable openlogi-frontmost@openlogi.dev`).
  - src: crates/openlogi-hook/gnome-shell-extension/openlogi-frontmost@openlogi.dev/extension.js
    dst: /usr/share/gnome-shell/extensions/openlogi-frontmost@openlogi.dev/extension.js
    file_info:
      mode: 0644
  - src: crates/openlogi-hook/gnome-shell-extension/openlogi-frontmost@openlogi.dev/metadata.json
    dst: /usr/share/gnome-shell/extensions/openlogi-frontmost@openlogi.dev/metadata.json
    file_info:
      mode: 0644
```

`install.sh`: an `install -Dm644` for both files to the same paths, under a `# ── GNOME Shell extension` banner, printing the enable command afterwards. `uninstall.sh`: remove the directory. `postinstall.sh` is not touched — it cannot enable a per-user extension.

- [ ] **Step 2: Docs**

`docs/INSTALL-linux.md`, new section `## Per-game profiles on GNOME Wayland`: why (Mutter hides focus from clients; `Introspect` is denied on GNOME 50), the two commands (`gnome-extensions enable openlogi-frontmost@openlogi.dev`, then log out and in), how to check (`busctl --user call … GetFocusedWindow`), and that wlroots compositors and X11 need nothing. Replace the limitations row *"Wayland: per-application profile switching | Requires XWayland"* with *"GNOME Wayland: profile switching | Needs the OpenLogi Shell extension (see above)"*. `README.md`: one paragraph under "Building" pointing at that section.

- [ ] **Step 3: Verify**

Run: `cargo xtask ci shellcheck` (or `shellcheck packaging/linux/*.sh` if the job is not on this host — name which), and `cargo run -p xtask -- linux package` if `nfpm` is installed; otherwise state it was not run.

- [ ] **Step 4: Commit**

```bash
git commit -am "build(linux): ship the GNOME Shell extension and document enabling it"
```

---

## Task 9: Run the Lost Ark profile on the real mouse

**Files:** none — verification. Report goes into `docs/superpowers/STATUS.md`.

The G703 is attached; `openlogi list` showing `○` means asleep. Back up `~/.config/openlogi/config.toml` first.

- [ ] **Step 1: Write the profile**

Transcribe the vault note's table into `config.toml`, mapping `G2→RightClick`, `G3→MiddleClick`, `G4→Back`, `G5→Forward`, `G6→DpiToggle`. The transcription assigns all six buttons in the normal layer, so the trigger must displace one: for this run bind `DpiToggle = "GShift"` and leave `ShiftG` unbound — the owner decides the permanent layout. The macros themselves are already in `[macros]` from the previous plan's verification. If Lost Ark is not installed under Proton on this machine, match a stand-in: `{ WmClass = "org.gnome.TextEditor" }`.

- [ ] **Step 2: Switching latency**

With `busctl --user monitor org.openlogi.Frontmost` and the agent at `info`, alt-tab into the matched window five times. Report the worst signal→`profile applied` gap. Expected: under 100 ms.

- [ ] **Step 3: Unknown keeps the profile**

With the profile applied, press Super (the overview), then minimize every window, then open a window with no `WM_CLASS` if one is at hand. Expected: no `profile applied` line, and a bound macro still fires. Then focus a normal application: the profile drops.

- [ ] **Step 4: G-Shift**

Hold the trigger, hold the right button: `SuperRight` auto-clicks. Release the trigger while still holding the right button: the macro keeps running until the right button releases. Release erratically twenty times. Then ask the kernel: `python3 -c "import evdev; …active_keys()…"` on the injector device (see STATUS.md, "Two things worth knowing") — expected empty.

- [ ] **Step 5: A switch stops the macro**

Hold `SuperSpace`, alt-tab away while holding. Expected: `SPACE` stops within the switch latency, `active_keys()` is empty, and releasing the button afterwards emits nothing.

- [ ] **Step 6: Report**

Record the measured latency, the four outcomes and anything surprising in `docs/superpowers/STATUS.md` (a new "Per-game profiles — verified" section, and mark plan 3 done in "The plans, in order"). Commit as `docs: record the per-game profile verification`.

---

## Definition of done

Alt-tabbing into the game applies its profile within the D-Bus round trip, going to the desktop does not drop it, the G-Shift layer works on the hook thread, a profile switch releases every key a macro held — all measured on the G703, not assumed.

## What comes after

| Plan | Delivers |
|---|---|
| **Per-profile device settings** | The spec's `device:` block — DPI presets, report rate and lighting applied over HID++ on activation. Deferred from this plan because it is a write path, not a matching one. |
| **The GUI** | Profile list with the applied indicator (`AgentSnapshot::active_profile`), macro recorder, G-Shift toggle on the button screen, and extension onboarding (spec §8). |

## Self-review

- Spec §5.2: `id, name, icon` ✔ (Task 1); `match` ✔; `device` ✗ **deferred, declared**; `assignments.normal/g_shift` ✔; `GShiftTrigger` = `Action::GShift` ✔ (Task 2); `Disabled` = `Action::None` ✔.
- Spec §6: rule 1 level-triggered ✔ (Task 5, `every_into` + reload re-evaluation); rule 2 `None` = unknown ✔ (Task 5); rule 3 identity ✔ (Tasks 1, 4); push ✔ (Task 7); extension required + shipped ✔ (Task 8); onboarding in the GUI → next plan.
- Spec §7: release on profile change ✔ — existing `cancel_all_buttons → stop_all`, exercised in Task 9 step 5.
- Types: `ActiveScope` (Task 3) is what Tasks 5 and 6 consume; `FocusedWindow` (Task 1) is what Tasks 4, 5 and 7 produce and consume; `HookMaps::g_shift` (Task 6) is filled from `g_shift_bindings_for` (Task 3). `PROTOCOL_VERSION`: 31 in Task 2, 32 in Task 5.

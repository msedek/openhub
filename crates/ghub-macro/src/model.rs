//! The macro data model: what a macro is, independent of how it runs.
//!
//! Step payloads are raw Linux input event codes (`KEY_*` and `BTN_*` from
//! `linux/input-event-codes.h`), the same numbers [`ghub_models`] uses for
//! button slots, so nothing has to translate between layers.
//!
//! [`ghub_models`]: https://docs.rs/ghub-models

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Stable identifier for a macro, unique within a configuration.
///
/// It is the key of the config's macro table and the payload a button binding
/// stores, so renaming a macro never invalidates a binding.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MacroId(pub String);

/// One instruction in a macro sequence.
///
/// Press and release are separate steps because the real macros need them:
/// "Hyper" is `Alt down, V tap, Alt up`, which a tap-only model cannot express.
/// The tap variants exist so the common case cannot be written unbalanced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// Press a key and leave it down.
    KeyDown(u16),
    /// Release a key.
    KeyUp(u16),
    /// Press and release a key.
    KeyTap(u16),
    /// Press a mouse button and leave it down.
    ButtonDown(u16),
    /// Release a mouse button.
    ButtonUp(u16),
    /// Press and release a mouse button.
    ButtonTap(u16),
    /// Wait before the next step.
    Delay {
        /// How long to wait, in milliseconds.
        millis: u32,
    },
}

/// How a macro repeats — G HUB's three modes, no more and no fewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    /// Run the sequence once per press.
    Once,
    /// Repeat the sequence every `interval_ms` for as long as the button is
    /// held. This is the mode every one of the owner's nine real macros uses.
    WhileHeld {
        /// Delay between the end of one run and the start of the next.
        interval_ms: u32,
    },
    /// Repeat the sequence every `interval_ms`, started by one press and
    /// stopped by the next.
    Toggle {
        /// Delay between the end of one run and the start of the next.
        interval_ms: u32,
    },
}

/// A named sequence of steps and the mode that decides how often it runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    /// The key this macro is stored and referenced under.
    pub id: MacroId,
    /// Human-readable name, shown in the GUI and in an action label.
    pub name: String,
    /// The sequence, executed in order.
    pub steps: Vec<Step>,
    /// How the sequence repeats.
    pub repeat: RepeatMode,
}

impl Macro {
    /// Every input code that one run of this macro could leave pressed.
    ///
    /// This is not a convenience. It is how the executor guarantees it releases
    /// everything without having to reason about arbitrary step sequences at
    /// cleanup time: a code that is pressed and released within the sequence is
    /// balanced and must *not* be reported, because emitting a release for it
    /// would inject a spurious up event.
    ///
    /// Keys and buttons share one code space in `EV_KEY`, so one set covers
    /// both. The result is sorted, so it is deterministic.
    #[must_use]
    pub fn held_codes(&self) -> Vec<u16> {
        let mut held = BTreeSet::new();

        for step in &self.steps {
            match *step {
                Step::KeyDown(code) | Step::ButtonDown(code) => {
                    held.insert(code);
                }
                Step::KeyUp(code) | Step::ButtonUp(code) => {
                    held.remove(&code);
                }
                Step::KeyTap(_) | Step::ButtonTap(_) | Step::Delay { .. } => {}
            }
        }

        held.into_iter().collect()
    }
}

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
    ///
    /// The fixture is a deliberately truncated "Hyper": the plan asserted this
    /// on the real one, but `Alt down, V tap, Alt up` is balanced and leaks
    /// nothing — it is the same shape as
    /// `a_balanced_down_and_up_is_not_held`, which asserts the opposite. The
    /// unbalanced sequence is the case this test is named for.
    #[test]
    fn held_codes_reports_every_code_a_run_could_leave_down() {
        let m = Macro {
            id: MacroId("half-hyper".into()),
            name: "Half Hyper".into(),
            steps: vec![Step::KeyDown(56), Step::KeyTap(47)],
            repeat: RepeatMode::WhileHeld { interval_ms: 25 },
        };

        assert_eq!(m.held_codes(), vec![56]);
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

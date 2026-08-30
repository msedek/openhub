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
            Self::SteamAppId(id) => {
                (window.steam_app_id == Some(*id)).then_some(Strength::SteamAppId)
            }
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
    /// Bindings that apply while the trigger is not held.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub normal: BTreeMap<ButtonId, Action>,
    /// Bindings that replace the ones above while the trigger is held.
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
    /// The button assignments the profile applies while it is active.
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
        let profiles = table(vec![(
            "lost-ark",
            profile(vec![MatchRule::WmClass("lostark.exe".into())]),
        )]);
        assert_eq!(
            resolve(&profiles, &window("LostArk.exe")).map(|id| id.0.as_str()),
            Some("lost-ark")
        );
        assert_eq!(resolve(&profiles, &window("steam")), None);
    }

    #[test]
    fn steam_app_id_matches_when_the_class_does_not() {
        let profiles = table(vec![(
            "lost-ark",
            profile(vec![MatchRule::SteamAppId(1_599_340)]),
        )]);
        let mut win = window("steam_app_1599340");
        assert_eq!(resolve(&profiles, &win), None);
        win.steam_app_id = Some(1_599_340);
        assert!(resolve(&profiles, &win).is_some());
    }

    #[test]
    fn title_is_a_regex_and_needs_a_title() {
        let profiles = table(vec![(
            "lost-ark",
            profile(vec![MatchRule::Title("^LOST ARK".into())]),
        )]);
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
            (
                "c-by-class",
                profile(vec![MatchRule::WmClass("game".into())]),
            ),
            ("a-by-title", profile(vec![MatchRule::Title(".*".into())])),
        ]);
        let mut win = window("game");
        win.title = Some("anything".into());
        assert_eq!(
            resolve(&profiles, &win).map(|id| id.0.as_str()),
            Some("c-by-class")
        );
        win.app.id = "other".into();
        assert_eq!(
            resolve(&profiles, &win).map(|id| id.0.as_str()),
            Some("a-by-title")
        );
    }

    #[test]
    fn round_trips_through_toml() {
        let mut profile = profile(vec![
            MatchRule::WmClass("lostark.exe".into()),
            MatchRule::SteamAppId(1_599_340),
            MatchRule::Title("^LOST ARK".into()),
        ]);
        profile.assignments.normal.insert(
            ButtonId::Back,
            Action::RunMacro(ghub_macro::MacroId("hyper".into())),
        );
        profile.assignments.g_shift.insert(
            ButtonId::RightClick,
            Action::RunMacro(ghub_macro::MacroId("superright".into())),
        );
        profile
            .assignments
            .normal
            .insert(ButtonId::DpiToggle, Action::None);
        let text = toml::to_string(&profile).expect("serialize");
        // `g_shift` here holds only a table-valued action, so the `toml`
        // crate folds the otherwise-empty `[assignments.g_shift]` header into
        // its one child rather than writing it separately — the child's own
        // section header is still the layer's proof.
        assert!(text.contains("[assignments.g_shift.RightClick]"), "{text}");
        let back: GameProfile = toml::from_str(&text).expect("parse");
        assert_eq!(back, profile);
    }
}

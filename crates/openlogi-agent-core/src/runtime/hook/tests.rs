//! Regression tests for OS-hook state and dispatch policy.

use super::*;
use openlogi_core::binding::{GESTURE_SWIPE_THRESHOLD, LongPressBinding};

fn token(id: u64, button: ButtonId) -> PressToken {
    PressToken::hook_for_test(id, button)
}

// The mid-swipe gate itself is unit-tested on `SwipeAccumulator` in
// `openlogi-core`; these cover only what `HoldState` adds on top — tagging a
// commit with the exact press and held button, and matching the release.

#[test]
fn accumulate_tags_a_committed_swipe_with_the_held_press() {
    let mut hold = HoldState::default();
    let press = token(1, ButtonId::Back);
    hold.begin(ButtonId::Back, press.clone());
    hold.swipe.backdate_hold_for_test();

    assert_eq!(
        hold.accumulate(GESTURE_SWIPE_THRESHOLD + 10, 0),
        Some((press.clone(), ButtonId::Back, GestureDirection::Right))
    );
    assert_eq!(
        hold.accumulate(50, 0),
        None,
        "commits at most once per hold"
    );
    assert_eq!(hold.end(ButtonId::Back), Some((press, false)));
}

#[test]
fn a_same_button_repress_restarts_the_stale_hold() {
    let mut hold = HoldState::default();
    let old = token(1, ButtonId::Back);
    assert!(matches!(
        hold.prepare_begin(ButtonId::Back),
        HoldAdmission::Begin
    ));
    hold.begin(ButtonId::Back, old);

    let replacement = token(2, ButtonId::Back);
    assert!(
        matches!(hold.prepare_begin(ButtonId::Back), HoldAdmission::Begin),
        "a same-button re-press is proof of a lost release"
    );
    hold.begin(ButtonId::Back, replacement.clone());
    hold.swipe.backdate_hold_for_test();
    assert_eq!(
        hold.accumulate(GESTURE_SWIPE_THRESHOLD + 10, 0),
        Some((replacement, ButtonId::Back, GestureDirection::Right))
    );
}

#[test]
fn an_aged_hold_yields_to_a_new_buttons_press() {
    let mut hold = HoldState::default();
    hold.begin(ButtonId::Back, token(1, ButtonId::Back));
    hold.backdate_for_test();

    let replacement = token(2, ButtonId::Forward);
    let HoldAdmission::Replace(stale) = hold.prepare_begin(ButtonId::Forward) else {
        panic!("an aged hold must yield to a new press");
    };
    assert_eq!(stale, token(1, ButtonId::Back));
    hold.begin(ButtonId::Forward, replacement.clone());
    hold.swipe.backdate_hold_for_test();
    assert_eq!(
        hold.accumulate(GESTURE_SWIPE_THRESHOLD + 10, 0),
        Some((replacement, ButtonId::Forward, GestureDirection::Right))
    );
}

#[test]
fn begin_is_first_wins_while_a_hold_is_active() {
    let mut hold = HoldState::default();
    let first = token(1, ButtonId::Back);
    hold.begin(ButtonId::Back, first.clone());
    hold.swipe.backdate_hold_for_test();
    assert!(
        matches!(hold.prepare_begin(ButtonId::Forward), HoldAdmission::Refuse),
        "a second press must not hijack the active hold"
    );

    assert_eq!(
        hold.accumulate(GESTURE_SWIPE_THRESHOLD + 10, 0),
        Some((first.clone(), ButtonId::Back, GestureDirection::Right))
    );
    assert_eq!(hold.end(ButtonId::Forward), None);
    assert_eq!(hold.end(ButtonId::Back), Some((first, false)));
}

#[test]
fn end_matches_the_held_button_and_returns_its_token() {
    let mut hold = HoldState::default();
    let press = token(1, ButtonId::Back);
    hold.begin(ButtonId::Back, press.clone());
    assert_eq!(hold.end(ButtonId::Forward), None);
    assert_eq!(hold.end(ButtonId::Back), Some((press, true)));
}

#[test]
fn resolve_gesture_click_prefers_explicit_then_falls_back_to_default() {
    let gestures = BTreeMap::from([(
        ButtonId::Back,
        BTreeMap::from([(GestureDirection::Click, Action::Copy)]),
    )]);
    assert_eq!(
        resolve_gesture_click(&gestures, ButtonId::Back),
        Action::Copy
    );

    let off = BTreeMap::from([(
        ButtonId::Back,
        BTreeMap::from([(GestureDirection::Click, Action::None)]),
    )]);
    assert_eq!(resolve_gesture_click(&off, ButtonId::Back), Action::None);
}

#[test]
fn fail_open_press_pairs_release() {
    let mut fail_open = HashSet::new();
    assert_eq!(
        remapped_press_disposition(ButtonId::Back, true, &mut fail_open),
        EventDisposition::Suppress
    );
    assert_eq!(
        remapped_release_disposition(ButtonId::Back, &mut fail_open),
        EventDisposition::Suppress
    );
    assert_eq!(
        remapped_press_disposition(ButtonId::Forward, false, &mut fail_open),
        EventDisposition::PassThrough
    );
    assert_eq!(
        remapped_release_disposition(ButtonId::Forward, &mut fail_open),
        EventDisposition::PassThrough
    );
    assert_eq!(
        remapped_release_disposition(ButtonId::Forward, &mut fail_open),
        EventDisposition::Suppress
    );
}

#[test]
fn rejected_key_edges_fail_open() {
    assert_eq!(queued_event_disposition(true), EventDisposition::Suppress);
    assert_eq!(
        queued_event_disposition(false),
        EventDisposition::PassThrough
    );
}

#[test]
fn scroll_interception_uses_the_button_source_safety_policy_and_skips_trackpads() {
    let logitech = EventDevice {
        vendor_id: Some(openlogi_hook::LOGITECH_VENDOR_ID),
        product_name: Some("Logitech MX Master".to_string()),
        ..EventDevice::default()
    };
    let trackpad = EventDevice {
        product_name: Some("Magic Trackpad".to_string()),
        ..EventDevice::default()
    };

    assert!(scroll_source_may_intercept(false, Some(&logitech)));
    assert!(!scroll_source_may_intercept(true, Some(&logitech)));
    assert!(!scroll_source_may_intercept(false, Some(&trackpad)));
    assert_eq!(
        scroll_source_may_intercept(false, None),
        !cfg!(target_os = "macos"),
        "only macOS requires callback-time device attribution"
    );
}

#[test]
fn rebound_horizontal_wheel_maps_to_thumbwheel_directions() {
    let maps = HookMaps {
        bindings: BTreeMap::from([
            (ButtonId::ThumbwheelScrollUp, Action::NextTab.into()),
            (ButtonId::ThumbwheelScrollDown, Action::PrevTab.into()),
        ]),
        gestures: BTreeMap::new(),
        g_shift: BTreeMap::new(),
    };
    assert_eq!(
        rebound_thumbwheel_action(&maps, 1.0),
        Some((ButtonId::ThumbwheelScrollDown, Action::PrevTab))
    );
    assert_eq!(
        rebound_thumbwheel_action(&maps, -1.0),
        Some((ButtonId::ThumbwheelScrollUp, Action::NextTab))
    );
    assert_eq!(rebound_thumbwheel_action(&maps, 0.0), None);
}

#[test]
fn native_thumbwheel_scroll_stays_os_native() {
    let maps = HookMaps {
        bindings: BTreeMap::from([
            (
                ButtonId::ThumbwheelScrollUp,
                default_binding(ButtonId::ThumbwheelScrollUp).into(),
            ),
            (
                ButtonId::ThumbwheelScrollDown,
                default_binding(ButtonId::ThumbwheelScrollDown).into(),
            ),
        ]),
        gestures: BTreeMap::new(),
        g_shift: BTreeMap::new(),
    };
    assert_eq!(rebound_thumbwheel_action(&maps, 1.0), None);
    assert_eq!(rebound_thumbwheel_action(&maps, -1.0), None);
}

#[test]
fn long_press_never_passes_through_as_a_native_click() {
    let binding = Binding::LongPress(LongPressBinding::new(
        default_binding(ButtonId::Back),
        Action::MissionControl,
    ));
    assert!(!binding_passes_through(ButtonId::Back, &binding));
}

/// The floor from `ButtonId::may_suppress`, seen from the gate that enforces
/// it: the hook decodes the primary click like any other button, but only
/// swallows it once the user put something in its place. A profile that
/// silenced it with `Action::None` would leave the desktop unclickable and no
/// way to reach the GUI that would undo the profile.
#[test]
fn the_primary_click_is_only_swallowed_when_something_is_bound_to_it() {
    assert!(binding_passes_through(
        ButtonId::LeftClick,
        &Binding::Single(Action::LeftClick)
    ));
    assert!(binding_passes_through(
        ButtonId::LeftClick,
        &Binding::Single(Action::None)
    ));
    assert!(!binding_passes_through(
        ButtonId::LeftClick,
        &Binding::Single(Action::Copy)
    ));
}

/// Every other button the hook decodes is swallowed as soon as it carries a
/// binding that is not its own native click — the gap this widening closes.
/// Before it, the button behind the wheel could not host a remap and the
/// secondary button could not become an auto-clicker.
#[test]
fn any_other_bound_button_is_swallowed() {
    for id in [
        ButtonId::RightClick,
        ButtonId::MiddleClick,
        ButtonId::Back,
        ButtonId::Forward,
        ButtonId::DpiToggle,
    ] {
        assert!(
            !binding_passes_through(id, &Binding::Single(Action::None)),
            "{id} passed through a deliberate Action::None"
        );
        assert!(
            !binding_passes_through(id, &Binding::Single(Action::Copy)),
            "{id} passed through a bound action"
        );
    }
}

#[test]
fn resolve_gesture_click_falls_back_when_click_is_absent() {
    let no_click = BTreeMap::from([(
        ButtonId::Back,
        BTreeMap::from([(GestureDirection::Up, Action::Copy)]),
    )]);
    assert_eq!(
        resolve_gesture_click(&no_click, ButtonId::Back),
        default_binding(ButtonId::Back)
    );

    let empty = BTreeMap::new();
    assert_eq!(
        resolve_gesture_click(&empty, ButtonId::Forward),
        default_binding(ButtonId::Forward)
    );
}

fn maps() -> HookMaps {
    let mut bindings = BTreeMap::new();
    bindings.insert(ButtonId::DpiToggle, Binding::Single(Action::GShift));
    bindings.insert(ButtonId::Back, Binding::Single(Action::Copy));
    bindings.insert(ButtonId::RightClick, Binding::Single(Action::RightClick));
    let mut g_shift = BTreeMap::new();
    g_shift.insert(ButtonId::RightClick, Binding::Single(Action::Paste));
    HookMaps {
        bindings,
        gestures: BTreeMap::new(),
        g_shift,
    }
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
    assert_eq!(
        maps.resolve(ButtonId::RightClick, false),
        Some(Binding::Single(Action::RightClick))
    );
    assert_eq!(
        maps.resolve(ButtonId::RightClick, true),
        Some(Binding::Single(Action::Paste))
    );
    assert_eq!(
        maps.resolve(ButtonId::Back, true),
        Some(Binding::Single(Action::Copy))
    );
}

#[test]
fn a_release_resolves_in_the_layer_its_press_used() {
    // Press under G-Shift, release after the trigger let go: the release must
    // still resolve shifted, or the press is suppressed and the release is
    // not — an OS-level stuck button.
    let mut shifted = HashSet::new();
    assert!(press_layer(true, true, &mut shifted, ButtonId::RightClick));
    assert!(press_layer(
        false,
        false,
        &mut shifted,
        ButtonId::RightClick
    ));
    assert!(
        !press_layer(false, false, &mut shifted, ButtonId::RightClick),
        "consumed"
    );
    assert!(!press_layer(true, false, &mut shifted, ButtonId::Back));
    assert!(
        !press_layer(false, true, &mut shifted, ButtonId::Back),
        "pressed unshifted, released shifted"
    );
}

#[test]
fn the_trigger_on_the_primary_button_is_swallowed() {
    // GShift is an explicit binding, so the left button's floor lets it be
    // suppressed — a trigger that also clicks would be useless.
    assert!(!binding_passes_through(
        ButtonId::LeftClick,
        &Binding::Single(Action::GShift)
    ));
}

#[test]
fn a_trigger_press_is_recognised_when_none_is_held() {
    let mut held = None;
    assert_eq!(
        trigger_transition(true, true, &mut held, ButtonId::DpiToggle),
        Some(true)
    );
    assert_eq!(held, Some(ButtonId::DpiToggle));
}

#[test]
fn the_held_triggers_release_is_recognised_even_if_the_map_moved_on() {
    // A config rebuild between press and release reassigned DpiToggle away
    // from GShift — the release must still close the layer it opened.
    let mut held = Some(ButtonId::DpiToggle);
    assert_eq!(
        trigger_transition(false, false, &mut held, ButtonId::DpiToggle),
        Some(false)
    );
    assert_eq!(held, None);
}

#[test]
fn a_second_trigger_press_while_one_is_held_is_an_ordinary_button() {
    let mut held = Some(ButtonId::DpiToggle);
    assert_eq!(
        trigger_transition(true, true, &mut held, ButtonId::RightClick),
        None
    );
    assert_eq!(
        held,
        Some(ButtonId::DpiToggle),
        "the first trigger keeps it"
    );
}

#[test]
fn a_release_of_an_unheld_button_the_map_now_calls_trigger_is_ordinary() {
    // RightClick was never the remembered trigger; a rebuild that just made
    // it one must not let its release be mistaken for a trigger release.
    let mut held = None;
    assert_eq!(
        trigger_transition(false, true, &mut held, ButtonId::RightClick),
        None
    );
    assert_eq!(held, None);
}

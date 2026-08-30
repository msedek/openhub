//! OS-hook capture and gesture interpretation.
//!
//! Installs the platform hook lazily, reads atomically published button maps,
//! and converts callback-thread mouse/key input into the shared action runtime.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use openlogi_core::binding::{
    Action, Binding, ButtonId, GestureDirection, SwipeAccumulator, default_binding,
};
use openlogi_core::config::{KeyModifiers, KeyTrigger};
use openlogi_hook::{
    EventDevice, EventDisposition, Hook, HookEvent, KeyEvent, MouseEvent, source_is_remappable,
};
use tracing::{info, warn};

use super::scroll::ScrollInputHandle;
use super::{ActionDispatcher, PressToken};
use crate::event_monitor::SharedEventMonitor;

/// The two button maps the OS-hook callback reads, kept behind ONE lock so a
/// config rebuild publishes both atomically — a press during an owner switch can
/// never see the new single-action bindings against the old gesture map (or vice
/// versa), and the common case reads one lock instead of two.
#[derive(Default)]
pub struct HookMaps {
    /// Per-button immediate or threshold binding — the non-gesture dispatch path.
    pub bindings: BTreeMap<ButtonId, Binding>,
    /// Per-direction maps for the OS-hook gesture buttons (Middle/Back/Forward in
    /// gesture mode), so a hold+swipe resolves to a bound action. The dedicated
    /// HID++ gesture button (0x00c3) uses the gesture watcher's separate map
    /// instead — it never reaches the OS hook.
    pub gestures: BTreeMap<ButtonId, BTreeMap<GestureDirection, Action>>,
    /// The active profile's G-Shift layer: only the buttons it changes. Read
    /// while `G_SHIFT_HELD` is set, falling back to `bindings`.
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

/// Shared, atomically-published [`HookMaps`], threaded between the config owner
/// (orchestrator), the OS-hook callback, and the gesture watcher.
pub type SharedHookMaps = Arc<RwLock<HookMaps>>;

/// Shared keyboard trigger→action map for the function-key remapper. Unlike
/// mouse bindings these are not per-app-profile (M1 scope — per the spec's
/// non-goals), so a single map suffices. Keyed by the config `KeyTrigger`
/// (keycode + modifiers).
pub type SharedKeyboardBindings = Arc<RwLock<BTreeMap<KeyTrigger, Action>>>;

/// Convert the hook-layer modifier state into the config-layer type (the two
/// live in different crates — core is leaf-level and duplicates the four
/// bools). Drop-in identity once the field names align.
fn convert_modifiers(m: openlogi_hook::KeyModifiers) -> KeyModifiers {
    KeyModifiers {
        shift: m.shift,
        control: m.control,
        option: m.option,
        command: m.command,
    }
}

/// Tracks which OS-hook button (Middle/Back/Forward) is mid-hold and defers the
/// swipe detection itself to a shared [`SwipeAccumulator`], which commits a swipe
/// *mid-motion* like the HID++ gesture-button path in `openlogi-hid`. This wrapper
/// adds only the button identity the accumulator doesn't track; a press that
/// never commits a direction is a plain click, fired on release.
/// A gesture hold this old is presumed stale — real hold+swipe interactions
/// finish in well under a second, and only a lost button-up (with no OS
/// interrupt to trigger [`HoldState::cancel`]) leaves one lingering.
const STALE_HOLD: Duration = Duration::from_secs(10);

#[derive(Default)]
struct HoldState {
    current: Option<GestureHold>,
    swipe: SwipeAccumulator,
}

struct GestureHold {
    button: ButtonId,
    started_at: Instant,
    press: PressToken,
}

enum HoldAdmission {
    Begin,
    Replace(PressToken),
    Refuse,
}

impl HoldState {
    /// Prepare a hold for `button`. With several gesture buttons the first live
    /// hold wins, so a second button cannot hijack accumulated motion. The
    /// caller obtains a fresh [`PressToken`] only after this admission step.
    ///
    /// Two presses recover a hold whose button-up was lost (nothing else ever
    /// clears it when the OS drops a release without an interrupt): a re-press
    /// of the held button itself — a button cannot be pressed while down, so
    /// this is proof the release was lost — and any press once the hold has
    /// aged past [`STALE_HOLD`], without which every other gesture button
    /// would stay refused indefinitely.
    fn prepare_begin(&mut self, button: ButtonId) -> HoldAdmission {
        let Some(held) = self.current.take() else {
            return HoldAdmission::Begin;
        };
        if held.button != button && held.started_at.elapsed() < STALE_HOLD {
            self.current = Some(held);
            return HoldAdmission::Refuse;
        }

        self.swipe.end();
        if held.button == button {
            HoldAdmission::Begin
        } else {
            HoldAdmission::Replace(held.press)
        }
    }

    /// Store the token returned by the accepted lifecycle `Down`.
    fn begin(&mut self, button: ButtonId, press: PressToken) {
        self.current = Some(GestureHold {
            button,
            started_at: Instant::now(),
            press,
        });
        self.swipe.begin();
    }

    /// Feed a pointer-move delta into the active hold, tagging a committed swipe
    /// with its exact press token and held button. Returns one commit per hold,
    /// or `None` while still too short, already fired, or not holding.
    fn accumulate(&mut self, dx: i32, dy: i32) -> Option<(PressToken, ButtonId, GestureDirection)> {
        let held = self.current.as_ref()?;
        self.swipe
            .accumulate(dx, dy)
            .map(|dir| (held.press.clone(), held.button, dir))
    }

    /// End the hold for `button`, returning its exact token and whether it was a
    /// click. A swipe returns `false`; a stray release returns `None`.
    fn end(&mut self, button: ButtonId) -> Option<(PressToken, bool)> {
        let held = self.current.take_if(|held| held.button == button)?;
        let was_click = self.swipe.end();
        Some((held.press, was_click))
    }

    /// Cancel any in-progress hold without firing anything — used when the OS
    /// interrupts capture. A dropped button-up would otherwise leave a stale hold
    /// that the next stray pointer move turns into a phantom swipe.
    fn cancel(&mut self) {
        self.current = None;
        self.swipe.end();
    }

    /// Age the current hold past the staleness horizon, so tests can exercise
    /// the lost-button-up recovery without sleeping.
    #[cfg(test)]
    fn backdate_for_test(&mut self) {
        if let Some(held) = &mut self.current
            && let Some(aged) = Instant::now().checked_sub(STALE_HOLD)
        {
            held.started_at = aged;
        }
    }
}

/// Whether the G-Shift trigger is held. One flag for the process, not per
/// hook thread: the layer is a property of the user's hand, and on Linux the
/// trigger and the buttons it modifies may arrive on different evdev threads
/// (a receiver and a cable are two devices).
static G_SHIFT_HELD: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// In-progress gesture hold, one instance per hook-callback thread: the
    /// single macOS tap thread, or — on Linux — one thread per device, so two
    /// mice never share a hold (a press on one can't hijack the other's swipe).
    /// Thread-local rather than a shared `Mutex` keeps the hot path lock-free and
    /// free of cross-thread contention on the freeze-sensitive callback.
    static HOLD: RefCell<HoldState> = RefCell::new(HoldState::default());
    /// What each held button's press decided, so its release can repeat that
    /// decision instead of re-deriving one from a map that may have changed in
    /// between — a profile switch, a config reload, or the G-Shift layer
    /// opening or closing mid-press. A release that disagrees with its own
    /// press is either a stuck auxiliary button (down, never up) or an up the
    /// app never saw a down for.
    static PRESS_DISPOSITIONS: RefCell<HashMap<ButtonId, EventDisposition>> =
        RefCell::new(HashMap::new());
    /// Function keys whose held action owns an accepted lifecycle. Repeated
    /// key-down events are auto-repeat, not replacement presses; their first
    /// matching key-up ends the lifecycle.
    static HELD_KEYS: RefCell<HashSet<u16>> = RefCell::new(HashSet::new());
    /// Buttons whose press resolved in the shifted layer, so their release
    /// resolves there too. Per thread, like `PRESS_DISPOSITIONS`: a press and
    /// its release come from the same device.
    static SHIFTED_PRESSES: RefCell<HashSet<ButtonId>> = RefCell::new(HashSet::new());
    /// The physical button whose press set `G_SHIFT_HELD`, remembered so its
    /// release always clears the flag — even if a config rebuild between the
    /// press and the release changed what `id` is bound to. `Cell` (not
    /// `RefCell`) suffices: a button's press and its release arrive on the
    /// same device thread, one at a time.
    static TRIGGER_HELD: Cell<Option<ButtonId>> = const { Cell::new(None) };
}

/// The layer a press resolves in — and, for a release, the layer its press
/// used, so the two dispositions always pair.
fn press_layer(
    pressed: bool,
    held: bool,
    shifted_presses: &mut HashSet<ButtonId>,
    id: ButtonId,
) -> bool {
    if pressed {
        if held {
            shifted_presses.insert(id);
        }
        held
    } else {
        shifted_presses.remove(&id)
    }
}

/// Whether this button event is a G-Shift trigger edge, and if so the new
/// value for `G_SHIFT_HELD`.
///
/// A press is the trigger only when the current map says so *and* no trigger
/// is already held — a second trigger press while one is down is an ordinary
/// button in the shifted layer, not a nested layer switch. A release is the
/// trigger only when it matches the physical button `held` remembers, so a
/// config rebuild between a press and its release can never re-decide which
/// layer the release belongs to (leaving the flag stuck, or the release
/// falling through as an ordinary — undispatched — button).
fn trigger_transition(
    pressed: bool,
    is_trigger_now: bool,
    held: &mut Option<ButtonId>,
    id: ButtonId,
) -> Option<bool> {
    if pressed {
        if is_trigger_now && held.is_none() {
            *held = Some(id);
            Some(true)
        } else {
            None
        }
    } else if *held == Some(id) {
        *held = None;
        Some(false)
    } else {
        None
    }
}

/// Whether a button event's physical source may be remapped/suppressed.
///
/// macOS attributes every CGEvent to an IOKit sender and fails closed: only
/// known Logitech non-trackpad devices are remappable, so the built-in
/// trackpad can never be swallowed. Linux/Windows often lack attribution
/// (`device: None`); those platforms already restrict which devices the hook
/// attaches to, so unknown sources stay remappable.
fn button_source_may_remap(device: Option<&EventDevice>) -> bool {
    match device {
        Some(d) => source_is_remappable(Some(d)),
        None => {
            // Attribution missing: safe on Linux/Windows (device selection is
            // upstream of the callback). On macOS fail closed — an unattributed
            // event is more likely a trackpad/system source than a Logi mouse.
            !cfg!(target_os = "macos")
        }
    }
}

/// Whether a wheel event may be replaced by host-side smooth output.
///
/// Native trackpad/pixel gestures always stay untouched. macOS additionally
/// requires a known Logitech sender; Linux and Windows perform device
/// selection before this callback and therefore admit their unattributed
/// wheel events through the same policy as button remapping.
fn scroll_source_may_intercept(from_trackpad: bool, device: Option<&EventDevice>) -> bool {
    !from_trackpad && button_source_may_remap(device)
}

/// Off-thread worker for bound actions so the tap callback never injects input.
fn spawn_action_worker(dispatcher: ActionDispatcher) -> mpsc::SyncSender<Action> {
    let (tx, rx) = mpsc::sync_channel::<Action>(64);
    let _ = thread::Builder::new()
        .name("openlogi-action".into())
        .spawn(move || {
            while let Ok(action) = rx.recv() {
                dispatcher.dispatch(&action, None);
            }
        });
    tx
}

/// Queue a bound action without blocking the tap callback. Returns `false` if
/// the queue is full (caller should fail open and pass the physical event).
fn try_queue_action(tx: &mpsc::SyncSender<Action>, action: Action) -> bool {
    if tx.try_send(action).is_err() {
        warn!("action queue full — dropping bound action to keep the input hook live");
        false
    } else {
        true
    }
}

/// Remap path for every button the OS hook decodes. Must stay lock-light and
/// non-blocking.
fn handle_button(
    id: ButtonId,
    pressed: bool,
    device: Option<&EventDevice>,
    hooks: &SharedHookMaps,
    dispatcher: &ActionDispatcher,
) -> EventDisposition {
    // Controls the hook never sees, and events from a source that may not be
    // remapped, leave immediately — before any lock is touched. Whether a
    // button the hook *does* see may be swallowed depends on its binding, and
    // that decision waits for `binding_passes_through` below.
    if !id.is_os_hook_button() || !button_source_may_remap(device) {
        return EventDisposition::PassThrough;
    }

    // The trigger is consumed here: it is a layer switch, not an action, and
    // it never reaches the dispatcher. `may_suppress` keeps the left-button
    // floor honest — an explicit GShift binding is something bound.
    // `TRIGGER_HELD` pins which physical button opened the layer, so its
    // release always closes it even if a config rebuild between the two
    // events changed what `id` is now bound to.
    let is_trigger_now = hooks.try_read().is_ok_and(|m| m.is_trigger(id));
    let trigger_edge = TRIGGER_HELD.with(|held| {
        let mut current = held.get();
        let edge = trigger_transition(pressed, is_trigger_now, &mut current, id);
        held.set(current);
        edge
    });
    if let Some(new_flag) = trigger_edge {
        G_SHIFT_HELD.store(new_flag, Ordering::Release);
        if new_flag {
            // A stranded shifted-press entry (from before this button became
            // the trigger) must not survive to mispair a later release.
            SHIFTED_PRESSES.with_borrow_mut(|presses| {
                presses.remove(&id);
            });
        }
        return if id.may_suppress(&Action::GShift) {
            EventDisposition::Suppress
        } else {
            EventDisposition::PassThrough
        };
    }
    let shifted = SHIFTED_PRESSES.with_borrow_mut(|presses| {
        press_layer(pressed, G_SHIFT_HELD.load(Ordering::Acquire), presses, id)
    });

    // `try_read` only: a blocking read on the tap thread freezes every pointer
    // event while a config rebuild holds the write lock. Fail open if unavailable.
    if pressed {
        let is_gesture = hooks.try_read().is_ok_and(|m| {
            m.gestures.contains_key(&id) && !(shifted && m.g_shift.contains_key(&id))
        });
        // A refused begin — a second gesture button pressed mid-hold — falls
        // through to the single-action path: the first hold wins and this press
        // still means its plain click.
        let admission = is_gesture.then(|| HOLD.with_borrow_mut(|h| h.prepare_begin(id)));
        if let Some(HoldAdmission::Begin | HoldAdmission::Replace(_)) = &admission {
            if let Some(HoldAdmission::Replace(stale)) = &admission {
                dispatcher.cancel_stale_hook_press(stale);
            }
            if let Some(press) = dispatcher.try_hook_button_down(id, None) {
                HOLD.with_borrow_mut(|h| h.begin(id, press));
                return PRESS_DISPOSITIONS.with_borrow_mut(|s| press_disposition(id, true, s));
            }
            return PRESS_DISPOSITIONS.with_borrow_mut(|s| press_disposition(id, false, s));
        }
    } else {
        // Drop the HOLD borrow before any queueing (re-entrancy freeze hazard).
        let ended = HOLD.with_borrow_mut(|h| h.end(id));
        if let Some((press, was_click)) = ended {
            if was_click {
                let action = hooks
                    .try_read()
                    .ok()
                    .map(|m| resolve_gesture_click(&m.gestures, id));
                if let Some(action) = action {
                    info!(button = %id, action = %action.label(), "gesture click → executing bound action");
                    dispatcher.try_dispatch_while_pressed(&press, &action);
                }
            }
            dispatcher.try_hook_button_up(id);
            // The hold's own press is what this release pairs with; the entry
            // it left behind is spent either way.
            return PRESS_DISPOSITIONS.with_borrow_mut(|s| release_disposition(id, false, s));
        }
    }

    // What the map says *right now*: the whole answer for a press, and only a
    // fallback for a release, which must pair with what its own press already
    // did rather than re-decide from a map a rebuild may have changed since.
    let binding = hooks.try_read().ok().and_then(|m| m.resolve(id, shifted));
    let remapped = binding
        .as_ref()
        .filter(|binding| !binding_passes_through(id, binding));

    if !pressed {
        // End the lifecycle before deciding anything. `try_hook_button_up` is
        // a no-op when the press opened none, and returning early instead is
        // how a suppressed press whose binding vanished mid-hold used to leave
        // its lifecycle — and any macro run it owned — outstanding forever.
        dispatcher.try_hook_button_up(id);
        return PRESS_DISPOSITIONS
            .with_borrow_mut(|s| release_disposition(id, remapped.is_none(), s));
    }
    let Some(binding) = remapped else {
        return PRESS_DISPOSITIONS.with_borrow_mut(|s| press_disposition(id, false, s));
    };
    info!(button = %id, action = %binding.click_action().label(), "button → handling binding");
    let suppressed = dispatcher.try_hook_button_down(id, Some(binding)).is_some();
    PRESS_DISPOSITIONS.with_borrow_mut(|s| press_disposition(id, suppressed, s))
}

/// Whether the hook must leave this button's physical event to the desktop.
///
/// Two reasons, both of which apply to a press and its release alike. The
/// binding may only reproduce what the button already does natively, in which
/// case suppressing and re-synthesising it would be a round trip through the
/// injector for nothing. Or the button's own floor may refuse to be swallowed
/// at all — [`ButtonId::may_suppress`], which is what keeps a primary click
/// with nothing bound to it working.
fn binding_passes_through(id: ButtonId, binding: &Binding) -> bool {
    if matches!(binding, Binding::LongPress(_)) {
        // A threshold binding is always something explicitly bound, and its
        // short action is only half of it, so neither reason applies.
        return false;
    }
    let action = binding.click_action();
    is_native_click(id, &action) || !id.may_suppress(&action)
}

/// Press of a button the hook decodes: suppress when the remap was accepted,
/// otherwise let the physical press reach the desktop. Either way the decision
/// is recorded under `id` for the release to repeat.
///
/// The two ways a press passes through — the action queue rejected the remap,
/// or the binding was never suppressible to begin with — are deliberately not
/// told apart: both leave the desktop holding this button down.
fn press_disposition(
    id: ButtonId,
    suppressed: bool,
    presses: &mut HashMap<ButtonId, EventDisposition>,
) -> EventDisposition {
    let disposition = if suppressed {
        EventDisposition::Suppress
    } else {
        EventDisposition::PassThrough
    };
    presses.insert(id, disposition);
    disposition
}

/// Release of a button the hook decodes: a release always repeats what its own
/// press decided, whatever a config rebuild did to the map in between.
///
/// Only a release whose press this thread never recorded — the hook started
/// while the button was already down — has nothing to pair with, and falls
/// back to `map_passes_through`, what the map says now.
fn release_disposition(
    id: ButtonId,
    map_passes_through: bool,
    presses: &mut HashMap<ButtonId, EventDisposition>,
) -> EventDisposition {
    presses.remove(&id).unwrap_or(if map_passes_through {
        EventDisposition::PassThrough
    } else {
        EventDisposition::Suppress
    })
}

/// Suppress only input accepted by an off-thread runtime. Rejected input must
/// fail open so the hook never swallows an edge it could not dispatch.
fn queued_event_disposition(queued: bool) -> EventDisposition {
    if queued {
        EventDisposition::Suppress
    } else {
        EventDisposition::PassThrough
    }
}

/// Feed an in-progress gesture hold; always pass motion through so the cursor moves.
fn handle_moved(
    delta_x: i32,
    delta_y: i32,
    hooks: &SharedHookMaps,
    dispatcher: &ActionDispatcher,
) -> EventDisposition {
    let commit = HOLD.with_borrow_mut(|h| h.accumulate(delta_x, delta_y));
    if let Some((press, button, dir)) = commit {
        let action = hooks.try_read().ok().map(|m| {
            m.gestures
                .get(&button)
                .and_then(|dirs| dirs.get(&dir).cloned())
                .unwrap_or_else(|| resolve_gesture_click(&m.gestures, button))
        });
        if let Some(action) = action {
            info!(button = %button, ?dir, action = %action.label(), "gesture swipe → executing bound action");
            dispatcher.try_dispatch_while_pressed(&press, &action);
        }
    }
    EventDisposition::PassThrough
}

/// Remap one function-key edge without blocking the hook callback.
fn handle_key(
    event: KeyEvent,
    bindings: &SharedKeyboardBindings,
    action_tx: &mpsc::SyncSender<Action>,
    dispatcher: &ActionDispatcher,
) -> EventDisposition {
    let KeyEvent {
        keycode,
        pressed,
        modifiers,
    } = event;
    if !pressed {
        return HELD_KEYS.with_borrow_mut(|keys| {
            if keys.remove(&keycode) {
                queued_event_disposition(dispatcher.try_hook_key_up(keycode))
            } else {
                EventDisposition::PassThrough
            }
        });
    }
    if HELD_KEYS.with_borrow(|keys| keys.contains(&keycode)) {
        return EventDisposition::Suppress;
    }
    let trigger = KeyTrigger {
        keycode,
        modifiers: convert_modifiers(modifiers),
    };
    let Some(action) = bindings
        .try_read()
        .ok()
        .and_then(|map| map.get(&trigger).cloned())
    else {
        return EventDisposition::PassThrough;
    };

    info!(keycode, action = %action.label(), "key → executing bound action");
    let queued = if action.held_combo().is_some() {
        let queued = dispatcher.try_hook_key_down(keycode, &action);
        if queued {
            HELD_KEYS.with_borrow_mut(|keys| {
                keys.insert(keycode);
            });
        }
        queued
    } else {
        try_queue_action(action_tx, action)
    };
    queued_event_disposition(queued)
}

/// Attempt to start the OS hook. Returns `None` if Accessibility is not
/// granted or on an unsupported platform — the app continues without crashing.
pub fn start(
    hooks: SharedHookMaps,
    keyboard_bindings: SharedKeyboardBindings,
    dispatcher: ActionDispatcher,
    scroll: ScrollInputHandle,
    monitor: SharedEventMonitor,
) -> Option<Hook> {
    if !Hook::has_accessibility() {
        warn!(
            "Accessibility not granted — events will not be captured. \
             Open System Settings → Privacy & Security → Accessibility."
        );
        return None;
    }

    // Actions never run on the tap callback thread (HID CGEventTap freeze hazard).
    let action_tx = spawn_action_worker(dispatcher.clone());

    // The per-hold pointer accumulator lives in the thread-local `HOLD`; the
    // callback must never block — see the freeze-hazard note in `macos.rs`.
    let result = Hook::start(move |event| match event {
        HookEvent::Mouse(event) => {
            monitor.record(&event);
            match event {
                MouseEvent::Button {
                    id,
                    pressed,
                    device,
                } => handle_button(id, pressed, device.as_ref(), &hooks, &dispatcher),
                MouseEvent::Moved { delta_x, delta_y } => {
                    handle_moved(delta_x, delta_y, &hooks, &dispatcher)
                }
                MouseEvent::CaptureInterrupted => {
                    HOLD.with_borrow_mut(HoldState::cancel);
                    HELD_KEYS.with_borrow_mut(HashSet::clear);
                    G_SHIFT_HELD.store(false, Ordering::Release);
                    SHIFTED_PRESSES.with_borrow_mut(HashSet::clear);
                    TRIGGER_HELD.with(|held| held.set(None));
                    dispatcher.cancel_hook_thread_buttons();
                    scroll.cancel_hooks();
                    EventDisposition::PassThrough
                }
                MouseEvent::Scroll {
                    delta,
                    from_trackpad,
                    device,
                } => {
                    #[cfg(target_os = "windows")]
                    if delta.y() == 0.0
                        && let Some((button, action)) = hooks
                            .try_read()
                            .ok()
                            .and_then(|maps| rebound_thumbwheel_action(&maps, delta.x()))
                    {
                        info!(button = %button, action = %action.label(), "native thumb wheel → executing bound action");
                        return queued_event_disposition(try_queue_action(&action_tx, action));
                    }
                    if scroll_source_may_intercept(from_trackpad, device.as_ref()) {
                        return queued_event_disposition(scroll.try_hook_scroll(delta));
                    }
                    EventDisposition::PassThrough
                }
            }
        }
        // Function-key remapper: ordinary actions remain one-shot, while a
        // HoldShortcut enters the same down/up/cancel lifecycle as a mouse
        // button. The active set pairs key-up even if modifier state or config
        // changes while the key is down.
        HookEvent::Key(event) => handle_key(event, &keyboard_bindings, &action_tx, &dispatcher),
    });

    match result {
        Ok(hook) => {
            info!("OS input hook installed");
            Some(hook)
        }
        Err(e) => {
            warn!(error = %e, "could not install OS input hook — events will not be captured");
            None
        }
    }
}

/// Resolve a native horizontal-wheel tick to a rebound thumb-wheel action.
/// The built-in horizontal-scroll defaults intentionally return `None` so the
/// physical wheel stays native unless the user changed that direction. On
/// Windows/MX Master 2S, positive `WM_MOUSEHWHEEL` delta is the physical
/// backward/down direction, so it maps to `ThumbwheelScrollDown`.
#[cfg(any(target_os = "windows", test))]
fn rebound_thumbwheel_action(maps: &HookMaps, delta_x: f64) -> Option<(ButtonId, Action)> {
    let button = if delta_x > 0.0 {
        ButtonId::ThumbwheelScrollDown
    } else if delta_x < 0.0 {
        ButtonId::ThumbwheelScrollUp
    } else {
        return None;
    };
    let action = maps.bindings.get(&button)?.click_action();
    (action != default_binding(button)).then_some((button, action))
}

/// The action a gesture button's plain (no-swipe) click should fire: its
/// explicit [`GestureDirection::Click`] entry — honoring an explicit
/// [`Action::None`] ("Do Nothing") — or the button's [`default_binding`] when
/// the gesture map has no `Click` key (a sparse / hand-edited map, or the button
/// left the gesture set mid-hold). The fallback guarantees a gesture button's
/// suppressed press is never swallowed into nothing.
fn resolve_gesture_click(
    gestures: &BTreeMap<ButtonId, BTreeMap<GestureDirection, Action>>,
    id: ButtonId,
) -> Action {
    gestures
        .get(&id)
        .and_then(|m| m.get(&GestureDirection::Click).cloned())
        .unwrap_or_else(|| default_binding(id))
}

/// Whether `action` is just `id`'s own native event — i.e. the button is mapped
/// to the very click (or extra-button press) it already produces. In that case
/// the hook should pass the event through to the OS rather than suppress and
/// re-synthesise it. For Back/Forward this keeps the genuine hardware button
/// 4/5 intact instead of round-tripping it through synthesis.
fn is_native_click(id: ButtonId, action: &Action) -> bool {
    matches!(
        (id, action),
        (ButtonId::LeftClick, Action::LeftClick)
            | (ButtonId::RightClick, Action::RightClick)
            | (ButtonId::MiddleClick, Action::MiddleClick)
            | (ButtonId::Back, Action::MouseBack)
            | (ButtonId::Forward, Action::MouseForward)
    )
}

#[cfg(test)]
mod tests;

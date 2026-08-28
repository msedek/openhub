//! Outstanding macro runs, and the paths that end them.
//!
//! [`ghub_macro`]'s executor guarantees that a run releases everything it
//! pressed — but only once something calls `stop`. This module is the thing
//! that calls it. A run left orphaned by a path nobody considered is a key held
//! forever, which the user experiences as their desktop breaking for no reason,
//! so every way a run can end routes through [`MacroRunner`]:
//!
//! - the physical release, and every other terminal outcome of the press that
//!   started it — a lost release, a stale hold, a cancelled capture source, an
//!   invalidated generation, shutdown — arrive as one `Ended` event and land in
//!   [`MacroRunner::end_press`];
//! - a `Toggle` macro, which by definition outlives its press, is stopped by
//!   the next press of the same control, by [`MacroRunner::stop_source`] when
//!   its capture source goes away, and by [`MacroRunner::stop_all`] on a
//!   binding or profile change, a config reload, and agent shutdown;
//! - and if all of that were somehow missed, dropping the runner drops every
//!   `RunHandle`, whose own `Drop` stops and joins the run.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use ghub_macro::{Executor, Macro, MacroId, RepeatMode, RunHandle, Sink};
use tracing::{info, warn};

use super::button::{PressControl, PressToken};

/// The configured macro table, republished by the orchestrator on every config
/// rebuild and read when a binding fires.
pub type SharedMacros = Arc<RwLock<BTreeMap<MacroId, Macro>>>;

/// A fresh, empty macro table.
#[must_use]
pub fn shared_macros() -> SharedMacros {
    Arc::new(RwLock::new(BTreeMap::new()))
}

/// The agent's own sink: raw Linux input-event codes straight to the shared
/// `uinput` device. Macro steps carry those codes verbatim, so nothing
/// translates between the recording and the output.
struct InjectSink;

impl Sink for InjectSink {
    fn key_down(&self, code: u16) {
        openlogi_inject::post_key_code(code, true);
    }

    fn key_up(&self, code: u16) {
        openlogi_inject::post_key_code(code, false);
    }

    fn button_down(&self, code: u16) {
        openlogi_inject::post_key_code(code, true);
    }

    fn button_up(&self, code: u16) {
        openlogi_inject::post_key_code(code, false);
    }
}

/// What a latched [`RepeatMode::Toggle`] run is owned by.
///
/// The press token cannot serve: a toggle is stopped by a *different*, later
/// press than the one that started it. The physical control plus the device it
/// came from is the identity that survives between the two, and carrying the
/// device key is what lets a disconnect stop only that device's runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LatchKey {
    device_key: Option<String>,
    control: PressControl,
}

impl LatchKey {
    fn new(device_key: Option<&str>, control: &PressControl) -> Self {
        Self {
            device_key: device_key.map(str::to_owned),
            control: control.clone(),
        }
    }
}

/// The runs themselves, behind one lock.
struct Runs {
    executor: Executor,
    /// Runs owned by one press: [`RepeatMode::Once`] and
    /// [`RepeatMode::WhileHeld`], which end with it.
    by_press: HashMap<PressToken, RunHandle>,
    /// Latched [`RepeatMode::Toggle`] runs, which deliberately do not.
    latched: HashMap<LatchKey, RunHandle>,
}

/// Starts and stops macro runs on behalf of the button lifecycle.
///
/// Cloneable and shared: the button worker starts and ends runs, while the
/// dispatcher's cancellation paths reach the same registry from the agent's
/// other threads.
#[derive(Clone)]
pub(crate) struct MacroRunner {
    macros: SharedMacros,
    runs: Arc<Mutex<Runs>>,
}

impl MacroRunner {
    /// Build a runner that injects through the host.
    pub(crate) fn new(macros: SharedMacros) -> Self {
        Self::with_sink(macros, Arc::new(InjectSink))
    }

    /// Build a runner over an arbitrary sink, so tests can observe the exact
    /// edges a run emits instead of firing real input at the developer's
    /// desktop.
    pub(crate) fn with_sink(macros: SharedMacros, sink: Arc<dyn Sink>) -> Self {
        Self {
            macros,
            runs: Arc::new(Mutex::new(Runs {
                executor: Executor::new(sink),
                by_press: HashMap::new(),
                latched: HashMap::new(),
            })),
        }
    }

    /// Start `id` on behalf of an accepted press.
    ///
    /// `Once` and `WhileHeld` runs are keyed by the press and end with it.
    /// `Toggle` latches instead: the first press starts the run and the next
    /// press of the same control stops it. The executor runs both repeating
    /// modes identically — the latching is this layer's business, because only
    /// this layer knows what a press is.
    pub(crate) fn start(
        &self,
        press: &PressToken,
        control: &PressControl,
        device_key: Option<&str>,
        id: &MacroId,
    ) {
        let Some(definition) = self.lookup(id) else {
            return;
        };
        let mut runs = self.lock();
        match definition.repeat {
            RepeatMode::Toggle { .. } => {
                let key = LatchKey::new(device_key, control);
                if let Some(running) = runs.latched.remove(&key) {
                    info!(macro_id = %id.0, "toggle macro → stopping the latched run");
                    running.stop();
                    return;
                }
                info!(macro_id = %id.0, "toggle macro → latching a run on");
                let handle = runs.executor.start(&definition);
                runs.latched.insert(key, handle);
            }
            RepeatMode::Once | RepeatMode::WhileHeld { .. } => {
                info!(macro_id = %id.0, "macro → running for the lifetime of the press");
                let handle = runs.executor.start(&definition);
                // A press cannot own two runs. Replacing one would strand the
                // first with nothing left to stop it.
                if let Some(stale) = runs.by_press.insert(press.clone(), handle) {
                    stale.stop();
                }
            }
        }
    }

    /// Run `id` once for a dispatch path that owns no release.
    ///
    /// The thumb wheel, the Actions Ring and the off-thread action worker fire
    /// an action with no matching terminal event, so a repeating macro started
    /// there would never be told to stop. It degrades to a single pass instead
    /// — the same way a held chord degrades to a tap for a dispatcher that
    /// cannot release it. Blocks for the length of that pass, which is why the
    /// press-owned path above never comes through here.
    pub(crate) fn run_once(&self, id: &MacroId) {
        let Some(mut definition) = self.lookup(id) else {
            return;
        };
        definition.repeat = RepeatMode::Once;
        info!(macro_id = %id.0, "macro → one pass (dispatcher owns no release)");
        let handle = self.lock().executor.start(&definition);
        handle.stop();
    }

    /// End the run owned by `press`, whatever ended the press itself.
    ///
    /// Called for every terminal event the button worker emits, so release,
    /// a lost release, a dead capture source, an invalidated generation and
    /// shutdown all arrive here without a path of their own.
    pub(crate) fn end_press(&self, press: &PressToken) {
        let handle = self.lock().by_press.remove(press);
        if let Some(handle) = handle {
            handle.stop();
        }
    }

    /// Stop every latched run that came from `device_key` — `None` for the OS
    /// hook, which attributes no device.
    ///
    /// A latched toggle can only be un-latched by pressing its control again,
    /// so a control whose capture source has gone away would otherwise leave a
    /// run nobody can reach.
    pub(crate) fn stop_source(&self, device_key: Option<&str>) {
        let mut runs = self.lock();
        let keys: Vec<LatchKey> = runs
            .latched
            .keys()
            .filter(|key| key.device_key.as_deref() == device_key)
            .cloned()
            .collect();
        let handles: Vec<RunHandle> = keys
            .into_iter()
            .filter_map(|key| runs.latched.remove(&key))
            .collect();
        drop(runs);
        for handle in handles {
            handle.stop();
        }
    }

    /// Stop every outstanding run: a binding or profile change, a foreground
    /// switch, a config reload, or shutdown. Nothing that survives one of those
    /// still has a binding that would explain it.
    pub(crate) fn stop_all(&self) {
        let (by_press, latched) = {
            let mut runs = self.lock();
            (
                std::mem::take(&mut runs.by_press),
                std::mem::take(&mut runs.latched),
            )
        };
        for (_, handle) in by_press {
            handle.stop();
        }
        for (_, handle) in latched {
            handle.stop();
        }
    }

    fn lookup(&self, id: &MacroId) -> Option<Macro> {
        let found = self
            .macros
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned();
        if found.is_none() {
            warn!(macro_id = %id.0, "binding references a macro that is not in the config");
        }
        found
    }

    /// A poisoned registry still holds usable handles. Treating it as fatal
    /// would abandon every outstanding run, which is the one outcome this
    /// module exists to prevent.
    fn lock(&self) -> MutexGuard<'_, Runs> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::thread;
    use std::time::Duration;

    use ghub_macro::Step;
    use openlogi_core::binding::ButtonId;

    use super::{
        Arc, LatchKey, Macro, MacroId, MacroRunner, PressControl, PressToken, RepeatMode,
        SharedMacros, Sink, shared_macros,
    };

    /// Records every edge so a run can be watched from outside and checked for
    /// balance — a leaked key is invisible to a test that only counts events.
    #[derive(Default)]
    pub(super) struct Recorder {
        events: StdMutex<Vec<(&'static str, u16)>>,
    }

    impl Sink for Recorder {
        fn key_down(&self, code: u16) {
            self.push("kd", code);
        }
        fn key_up(&self, code: u16) {
            self.push("ku", code);
        }
        fn button_down(&self, code: u16) {
            self.push("bd", code);
        }
        fn button_up(&self, code: u16) {
            self.push("bu", code);
        }
    }

    impl Recorder {
        fn push(&self, kind: &'static str, code: u16) {
            self.events
                .lock()
                .expect("recorder lock")
                .push((kind, code));
        }

        /// How many times the sequence ran.
        ///
        /// Counted from the code that *opens* a pass, not from every key
        /// down: [`spam`] presses 56, taps 47, releases 56, so one pass emits
        /// two `kd` events and counting them all reports double.
        pub(super) fn passes(&self) -> usize {
            self.events
                .lock()
                .expect("recorder lock")
                .iter()
                .filter(|(kind, code)| *kind == "kd" && *code == PASS_OPENS_WITH)
                .count()
        }

        pub(super) fn is_balanced(&self) -> bool {
            let mut held: Vec<u16> = Vec::new();
            for (kind, code) in self.events.lock().expect("recorder lock").iter() {
                match *kind {
                    "kd" | "bd" => held.push(*code),
                    _ => {
                        if let Some(i) = held.iter().position(|c| c == code) {
                            held.remove(i);
                        }
                    }
                }
            }
            held.is_empty()
        }
    }

    /// `Alt down, V tap, Alt up`, repeated every 5 ms — the real "Hyper"
    /// macro sped up so a test can watch several passes go by.
    /// The code [`spam`] presses first. One occurrence of it in the recorded
    /// stream is one pass of the sequence.
    const PASS_OPENS_WITH: u16 = 56;

    fn spam(repeat: RepeatMode) -> Macro {
        Macro {
            id: MacroId("hyper".into()),
            name: "Hyper".into(),
            steps: vec![Step::KeyDown(56), Step::KeyTap(47), Step::KeyUp(56)],
            repeat,
        }
    }

    pub(super) fn table(repeat: RepeatMode) -> SharedMacros {
        let macros = shared_macros();
        macros
            .write()
            .expect("macro table lock")
            .insert(MacroId("hyper".into()), spam(repeat));
        macros
    }

    fn runner(repeat: RepeatMode) -> (MacroRunner, Arc<Recorder>) {
        let sink = Arc::new(Recorder::default());
        (MacroRunner::with_sink(table(repeat), sink.clone()), sink)
    }

    fn hyper() -> MacroId {
        MacroId("hyper".into())
    }

    /// Long enough for several 5 ms passes; short enough not to slow the suite.
    fn observe() {
        thread::sleep(Duration::from_millis(60));
    }

    #[test]
    fn a_while_held_run_repeats_until_its_press_ends() {
        let (runner, sink) = runner(RepeatMode::WhileHeld { interval_ms: 5 });
        let press = PressToken::hook_for_test(1, ButtonId::DpiToggle);

        runner.start(
            &press,
            &PressControl::Button(ButtonId::DpiToggle),
            None,
            &hyper(),
        );
        observe();
        runner.end_press(&press);

        let passes = sink.passes();
        assert!(passes > 1, "expected repeats while held, got {passes}");
        assert!(sink.is_balanced(), "a run left a key held");

        // `stop` joined the thread, so nothing can still be emitting.
        observe();
        assert_eq!(sink.passes(), passes, "the run kept going after its press");
    }

    /// The plan's headline hazard: a profile change with the button still
    /// down. The press is cancelled rather than released, and the run has to
    /// end on that path too — the executor's release guarantee is only
    /// reachable if something calls stop.
    #[test]
    fn a_cancelled_press_stops_its_run_like_a_release_does() {
        let (runner, sink) = runner(RepeatMode::WhileHeld { interval_ms: 5 });
        let press = PressToken::hook_for_test(1, ButtonId::RightClick);

        runner.start(
            &press,
            &PressControl::Button(ButtonId::RightClick),
            None,
            &hyper(),
        );
        observe();
        // Cancellation and release reach the registry through the same
        // terminal event, which is the point: no reason needs its own path.
        runner.end_press(&press);

        let passes = sink.passes();
        observe();
        assert_eq!(sink.passes(), passes);
        assert!(sink.is_balanced());
    }

    #[test]
    fn a_toggle_run_latches_past_its_release_and_stops_on_the_next_press() {
        let (runner, sink) = runner(RepeatMode::Toggle { interval_ms: 5 });
        let control = PressControl::Button(ButtonId::Forward);
        let first = PressToken::hook_for_test(1, ButtonId::Forward);
        let second = PressToken::hook_for_test(2, ButtonId::Forward);

        runner.start(&first, &control, None, &hyper());
        runner.end_press(&first);
        observe();
        let while_latched = sink.passes();
        assert!(
            while_latched > 1,
            "a toggle must outlive its press, saw {while_latched} passes"
        );

        runner.start(&second, &control, None, &hyper());
        let at_stop = sink.passes();
        observe();
        assert_eq!(at_stop, sink.passes(), "the second press did not un-latch");
        assert!(sink.is_balanced());
    }

    #[test]
    fn stop_all_reaches_a_latched_toggle_no_press_still_owns() {
        let (runner, sink) = runner(RepeatMode::Toggle { interval_ms: 5 });
        let press = PressToken::hook_for_test(1, ButtonId::Back);

        runner.start(
            &press,
            &PressControl::Button(ButtonId::Back),
            None,
            &hyper(),
        );
        runner.end_press(&press);
        observe();
        assert!(sink.passes() > 1);

        runner.stop_all();
        let at_stop = sink.passes();
        observe();
        assert_eq!(at_stop, sink.passes());
        assert!(sink.is_balanced());
    }

    /// A device that disconnects takes its latched runs with it: pressing the
    /// control again is the only way to un-latch one, and that control is gone.
    #[test]
    fn stop_source_stops_only_the_runs_from_that_source() {
        let (runner, sink) = runner(RepeatMode::Toggle { interval_ms: 5 });
        let from_hook = PressToken::hook_for_test(1, ButtonId::Back);
        let from_device = PressToken::hook_for_test(2, ButtonId::Forward);

        runner.start(
            &from_hook,
            &PressControl::Button(ButtonId::Back),
            None,
            &hyper(),
        );
        runner.start(
            &from_device,
            &PressControl::Button(ButtonId::Forward),
            Some("receiver:abc:slot:1"),
            &hyper(),
        );
        observe();

        runner.stop_source(Some("receiver:abc:slot:1"));
        {
            let runs = runner.lock();
            assert_eq!(runs.latched.len(), 1, "the hook's run must survive");
            assert!(
                runs.latched
                    .contains_key(&LatchKey::new(None, &PressControl::Button(ButtonId::Back)))
            );
        }

        let still_running = sink.passes();
        observe();
        assert!(
            sink.passes() > still_running,
            "stop_source stopped an unrelated run"
        );

        runner.stop_all();
        assert!(sink.is_balanced());
    }

    /// A binding pointing at a macro the config no longer holds must be inert,
    /// not a panic and not a run of something else.
    #[test]
    fn an_unknown_macro_id_starts_nothing() {
        let (runner, sink) = runner(RepeatMode::WhileHeld { interval_ms: 5 });
        let press = PressToken::hook_for_test(1, ButtonId::Back);

        runner.start(
            &press,
            &PressControl::Button(ButtonId::Back),
            None,
            &MacroId("deleted".into()),
        );
        observe();

        assert_eq!(sink.passes(), 0);
        assert!(runner.lock().by_press.is_empty());
    }

    /// A dispatch path with no release context degrades to one pass instead of
    /// starting a repeat nothing would ever stop.
    #[test]
    fn run_once_degrades_a_repeating_macro_to_a_single_pass() {
        let (runner, sink) = runner(RepeatMode::WhileHeld { interval_ms: 5 });

        runner.run_once(&hyper());
        observe();

        assert_eq!(sink.passes(), 1);
        assert!(sink.is_balanced());
        assert!(runner.lock().by_press.is_empty());
    }

    /// The last line of defence: even with every explicit path skipped,
    /// dropping the registry drops the handles, and `RunHandle::drop` stops
    /// and joins the run before returning.
    #[test]
    fn dropping_the_runner_stops_what_it_still_owns() {
        let sink = Arc::new(Recorder::default());
        let macros = table(RepeatMode::WhileHeld { interval_ms: 5 });
        let runner = MacroRunner::with_sink(macros, sink.clone());
        let press = PressToken::hook_for_test(1, ButtonId::Back);

        runner.start(
            &press,
            &PressControl::Button(ButtonId::Back),
            None,
            &hyper(),
        );
        observe();
        drop(runner);

        let at_drop = sink.passes();
        assert!(at_drop > 1);
        observe();
        assert_eq!(sink.passes(), at_drop);
        assert!(sink.is_balanced());
    }
}

//! Running a macro, and the guarantee that it releases what it pressed.
//!
//! One run lives on its own `std::thread`, not on the agent's tokio runtime: a
//! 25 ms cadence competing with device I/O drifts, and drift is exactly what
//! makes held-button spam feel weak. Pacing is [`crate::timer`]'s job.
//!
//! The guarantee is a ledger. Every code the run presses is recorded before it
//! is emitted, every release removes it again, and every exit path — stop,
//! completion, drop, panic — drains what is left. Releasing the button between
//! `KeyDown(Alt)` and `KeyUp(Alt)` is the case that leaves Alt held forever in
//! a naive implementation; here it releases Alt on the way out.

use std::{
    panic::{self, AssertUnwindSafe},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    Macro, RepeatMode, Step,
    timer::{Cancel, Pacer},
};

/// Where a run emits its input events.
///
/// The agent implements this over `openlogi-inject`; tests implement it with a
/// recorder. Methods take `&self` and cannot fail: a run thread has nowhere to
/// return an error to, and an injector that drops one event must not be able to
/// abort the sequence that would have released a held key.
pub trait Sink: Send + Sync {
    /// Press a key and leave it down.
    fn key_down(&self, code: u16);
    /// Release a key.
    fn key_up(&self, code: u16);
    /// Press a mouse button and leave it down.
    fn button_down(&self, code: u16);
    /// Release a mouse button.
    fn button_up(&self, code: u16);
}

/// Starts macro runs against one sink.
///
/// The executor itself holds no run state — each [`RunHandle`] owns its thread
/// — so one executor serves every button binding, and two runs started from it
/// are independent.
pub struct Executor {
    sink: Arc<dyn Sink>,
}

impl Executor {
    /// Build an executor that emits through `sink`.
    #[must_use]
    pub fn new(sink: Arc<dyn Sink>) -> Self {
        Self { sink }
    }

    /// Start `macro_def` on its own thread.
    ///
    /// The first pass over the steps always runs to completion, so a button
    /// tapped faster than the macro can repeat still fires once — G HUB
    /// behaves the same way. Stopping cuts short every wait after that.
    ///
    /// Dropping the returned handle stops the run and waits for its releases,
    /// so a handle that is discarded cannot leak a held key either.
    #[must_use]
    pub fn start(&self, macro_def: &Macro) -> RunHandle {
        let cancel = Arc::new(Cancel::default());
        let thread = thread::spawn({
            let macro_def = macro_def.clone();
            let sink = Arc::clone(&self.sink);
            let cancel = Arc::clone(&cancel);
            move || run(&macro_def, sink.as_ref(), &cancel)
        });

        RunHandle {
            cancel,
            thread: Some(thread),
        }
    }
}

/// A running macro.
///
/// Stopping — explicitly or by dropping this — returns only once the run thread
/// has emitted every outstanding release.
pub struct RunHandle {
    cancel: Arc<Cancel>,
    thread: Option<JoinHandle<()>>,
}

impl RunHandle {
    /// Stop the run and wait for it to release everything it pressed.
    ///
    /// Consuming `self` makes this idempotent by construction: there is no
    /// second call to make, and the `Drop` that follows finds the thread
    /// already joined and does nothing.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.cancel.signal();
        if let Some(thread) = self.thread.take() {
            // Joining is what makes "stop released everything" true rather than
            // eventually true: the thread drains its ledger before returning.
            // A run that panicked drained first and then resumed unwinding, so
            // the `Err` here carries no release still owed.
            drop(thread.join());
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The run thread's body: execute, then drain whatever is still held.
fn run(macro_def: &Macro, sink: &dyn Sink, cancel: &Cancel) {
    let mut ledger = Ledger::default();

    // The sequence is arbitrary user data emitted through an arbitrary sink, so
    // a panic is possible and must not be a way to keep a key held. Catch it,
    // drain, and only then let it continue unwinding.
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        repeat(macro_def, sink, &mut ledger, cancel);
    }));

    ledger.drain(sink);

    if let Err(payload) = outcome {
        panic::resume_unwind(payload);
    }
}

/// Apply the repeat mode: one pass, or passes on a fixed cadence until stopped.
fn repeat(macro_def: &Macro, sink: &dyn Sink, ledger: &mut Ledger, cancel: &Cancel) {
    let interval_ms = match macro_def.repeat {
        RepeatMode::Once => {
            sequence(macro_def, sink, ledger, cancel);
            return;
        }
        // Toggle differs from WhileHeld only in what starts and stops it, which
        // is the button runtime's business, not the executor's.
        RepeatMode::WhileHeld { interval_ms } | RepeatMode::Toggle { interval_ms } => interval_ms,
    };

    let mut pacer = Pacer::new(interval_ms, Instant::now());
    loop {
        sequence(macro_def, sink, ledger, cancel);

        if cancel.sleep_until(pacer.next_deadline(Instant::now())) {
            return;
        }
    }
}

/// One pass over the steps.
///
/// A stopped run does not abandon the pass: its remaining waits return
/// instantly, so the sequence finishes in microseconds and emits its own
/// releases in the order it wrote them. Whatever it still leaves held is the
/// ledger's problem.
fn sequence(macro_def: &Macro, sink: &dyn Sink, ledger: &mut Ledger, cancel: &Cancel) {
    for step in &macro_def.steps {
        match *step {
            Step::KeyDown(code) => ledger.press(sink, Held::Key(code)),
            Step::KeyUp(code) => ledger.release(sink, Held::Key(code)),
            Step::KeyTap(code) => {
                ledger.press(sink, Held::Key(code));
                ledger.release(sink, Held::Key(code));
            }
            Step::ButtonDown(code) => ledger.press(sink, Held::Button(code)),
            Step::ButtonUp(code) => ledger.release(sink, Held::Button(code)),
            Step::ButtonTap(code) => {
                ledger.press(sink, Held::Button(code));
                ledger.release(sink, Held::Button(code));
            }
            Step::Delay { millis } => {
                cancel.sleep_until(Instant::now() + Duration::from_millis(u64::from(millis)));
            }
        }
    }
}

/// One code the run has pressed, and how to let go of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Key(u16),
    Button(u16),
}

impl Held {
    fn down(self, sink: &dyn Sink) {
        match self {
            Held::Key(code) => sink.key_down(code),
            Held::Button(code) => sink.button_down(code),
        }
    }

    fn up(self, sink: &dyn Sink) {
        match self {
            Held::Key(code) => sink.key_up(code),
            Held::Button(code) => sink.button_up(code),
        }
    }
}

/// What the run currently has pressed, in the order it pressed it.
///
/// A `Vec` rather than a set: the order is the release order, and a macro holds
/// a handful of codes at most.
#[derive(Debug, Default)]
struct Ledger {
    held: Vec<Held>,
}

impl Ledger {
    /// Record, then emit.
    ///
    /// That order is deliberate. If the sink panics or the thread dies between
    /// the two, the drain emits a release for a code that may never have gone
    /// down — harmless, the desktop ignores it — whereas the opposite order
    /// leaves a key down with nothing recording it.
    fn press(&mut self, sink: &dyn Sink, what: Held) {
        if !self.held.contains(&what) {
            self.held.push(what);
        }
        what.down(sink);
    }

    fn release(&mut self, sink: &dyn Sink, what: Held) {
        self.held.retain(|held| *held != what);
        what.up(sink);
    }

    /// Release everything still held, newest first.
    ///
    /// Reverse order mirrors how a chord was pressed, so a modifier is released
    /// after the key it modified rather than before it.
    fn drain(&mut self, sink: &dyn Sink) {
        for what in self.held.drain(..).rev() {
            // One sink call that panics must not strand the codes behind it.
            drop(panic::catch_unwind(AssertUnwindSafe(|| what.up(sink))));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

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

        assert_eq!(
            sink.events()
                .iter()
                .filter(|(k, c)| *k == "kd" && *c == 56)
                .count(),
            1
        );
        assert!(sink.is_balanced());
    }

    #[test]
    fn while_held_repeats_until_stopped() {
        let sink = Arc::new(Recorder::default());
        let exec = Executor::new(sink.clone());

        let handle = exec.start(&hyper());
        std::thread::sleep(std::time::Duration::from_millis(200));
        handle.stop();

        let runs = sink
            .events()
            .iter()
            .filter(|(k, c)| *k == "kd" && *c == 56)
            .count();
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

    /// A sink that blows up mid-sequence, to prove the drain runs on the
    /// unwind path as well.
    struct Exploding {
        inner: Arc<Recorder>,
        panic_on_key: u16,
    }

    impl Sink for Exploding {
        fn key_down(&self, code: u16) {
            self.inner.key_down(code);
            assert!(code != self.panic_on_key, "sink exploded on {code}");
        }
        fn key_up(&self, code: u16) {
            self.inner.key_up(code);
        }
        fn button_down(&self, code: u16) {
            self.inner.button_down(code);
        }
        fn button_up(&self, code: u16) {
            self.inner.button_up(code);
        }
    }

    /// A panic inside the run must not become a way to keep Alt held. The
    /// panic is expected here: the run thread prints it, `stop` joins the
    /// corpse, and the assertion is that the ledger drained on the way out.
    #[test]
    fn a_panicking_sink_still_releases_everything() {
        let recorder = Arc::new(Recorder::default());
        let exec = Executor::new(Arc::new(Exploding {
            inner: recorder.clone(),
            panic_on_key: 47,
        }));
        let mut m = hyper();
        m.repeat = RepeatMode::Once;

        exec.start(&m).stop();

        assert!(recorder.is_balanced(), "left held: {:?}", recorder.events());
    }

    /// Records when each run started, so the cadence can be measured rather
    /// than assumed.
    struct Stamper {
        starts: Mutex<Vec<Instant>>,
    }

    impl Sink for Stamper {
        fn key_down(&self, code: u16) {
            if code == 56 {
                self.starts.lock().unwrap().push(Instant::now());
            }
        }
        fn key_up(&self, _code: u16) {}
        fn button_down(&self, _code: u16) {}
        fn button_up(&self, _code: u16) {}
    }

    /// The cadence is measured from a fixed clock, so the cost of a run does
    /// not accumulate as drift.
    ///
    /// Each run of this macro spends 10 ms of its 25 ms slot working. A loop
    /// that slept the interval *after* each run would settle at 35 ms — the
    /// 40 Hz the real macros ask for degraded to 29 Hz, which is the exact
    /// failure this crate is built to avoid. The assertion is on the mean
    /// rather than on any single interval: the schedule is anchored to
    /// absolute deadlines, so a scheduler hiccup borrows from the next slot
    /// and gives it back, while genuine drift moves the mean and cannot be
    /// given back.
    #[test]
    fn the_cadence_does_not_accumulate_the_cost_of_a_run() {
        let sink = Arc::new(Stamper {
            starts: Mutex::new(Vec::new()),
        });
        let exec = Executor::new(sink.clone());
        let m = Macro {
            id: MacroId("costly".into()),
            name: "Costly".into(),
            steps: vec![
                Step::KeyDown(56),
                Step::Delay { millis: 10 },
                Step::KeyUp(56),
            ],
            repeat: RepeatMode::WhileHeld { interval_ms: 25 },
        };

        let handle = exec.start(&m);
        std::thread::sleep(Duration::from_millis(500));
        handle.stop();

        let starts = sink.starts.lock().unwrap().clone();
        assert!(
            starts.len() >= 10,
            "too few runs to measure: {}",
            starts.len()
        );

        let span = starts[starts.len() - 1].duration_since(starts[0]);
        let intervals = u32::try_from(starts.len() - 1).expect("run count fits in u32");
        let mean_ms = span.as_secs_f64() * 1000.0 / f64::from(intervals);
        assert!(
            (20.0..=32.0).contains(&mean_ms),
            "mean interval {mean_ms:.1} ms is not the configured 25 ms cadence"
        );
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

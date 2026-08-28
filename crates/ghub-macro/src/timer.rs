//! Pacing and cancellation for a macro run.
//!
//! Two jobs, both about time: stopping a run the instant its button comes up,
//! and holding a cadence that does not drift.
//!
//! **Why not `timerfd`.** The obvious Linux answer is a `timerfd` armed with an
//! absolute deadline, polled together with an `eventfd` for the stop signal.
//! That is two file descriptors, an `epoll` loop and a second implementation
//! for every other platform, and it buys exactly one property: the interval is
//! measured from a fixed clock instead of accumulating each run's execution
//! time. [`Pacer`] gets that property from arithmetic — every deadline is
//! computed from the previous *deadline*, never from "now" — and
//! [`Cancel::sleep_until`] waits on a condition variable, which wakes on the
//! stop signal exactly as `epoll` would. Same drift resistance, same stop
//! latency, no platform code.

use std::{
    sync::{Condvar, Mutex, PoisonError},
    time::{Duration, Instant},
};

/// A zero interval would spin a core at whatever rate the injector can absorb,
/// so the smallest cadence a configuration can ask for is 1 ms (1 kHz), far
/// above the 40 Hz the real macros use.
const MIN_INTERVAL: Duration = Duration::from_millis(1);

/// The stop signal shared between a run thread and its handle.
///
/// Signalling wakes the thread out of a delay or an inter-run wait immediately,
/// which is what makes a 5-second `Delay` step releasable in microseconds
/// rather than in five seconds.
#[derive(Debug, Default)]
pub(crate) struct Cancel {
    stopped: Mutex<bool>,
    woken: Condvar,
}

impl Cancel {
    /// Ask the run to stop, and wake it if it is waiting.
    pub(crate) fn signal(&self) {
        let mut stopped = self.lock();
        *stopped = true;
        // notify_all, not notify_one: the guarantee must not depend on there
        // being exactly one waiter.
        self.woken.notify_all();
    }

    /// Wait until `deadline`, returning as soon as the stop signal arrives.
    ///
    /// Returns `true` when it returned because of the signal — including when
    /// the signal had already been raised before the call, so a cancelled run
    /// never sleeps again.
    pub(crate) fn sleep_until(&self, deadline: Instant) -> bool {
        let timeout = deadline.saturating_duration_since(Instant::now());
        let (stopped, _) = self
            .woken
            .wait_timeout_while(self.lock(), timeout, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *stopped
    }

    /// A poisoned stop flag still carries a usable `bool`; treating it as fatal
    /// would abandon a run thread mid-sequence, which is the one outcome this
    /// crate exists to prevent.
    fn lock(&self) -> std::sync::MutexGuard<'_, bool> {
        self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A fixed-clock cadence for the repeating modes.
///
/// Deadlines advance from the previous deadline, so the interval is the period
/// between run *starts* and the cost of a run does not push the next one later.
/// Naive `sleep(interval)` after each run accumulates that cost as drift, and
/// 25 ms drifting to 35 ms is the difference between spam that works and spam
/// that feels weak.
#[derive(Debug)]
pub(crate) struct Pacer {
    interval: Duration,
    next: Instant,
}

impl Pacer {
    /// Start a cadence of `interval_ms` from `start`, the instant the first run
    /// began.
    pub(crate) fn new(interval_ms: u32, start: Instant) -> Self {
        Self {
            interval: Duration::from_millis(u64::from(interval_ms)).max(MIN_INTERVAL),
            next: start,
        }
    }

    /// When the next run is due, given the current instant.
    ///
    /// `now` is a parameter rather than a `Instant::now()` call inside so the
    /// schedule is a pure function of its inputs and its tests cannot race the
    /// clock.
    ///
    /// If the previous run overran its slot the schedule resynchronises to
    /// `now` instead of firing a burst of back-to-back runs to catch up: the
    /// user asked for a rate, not for a quota.
    pub(crate) fn next_deadline(&mut self, now: Instant) -> Instant {
        self.next += self.interval;
        self.next = self.next.max(now);
        self.next
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Cancel, Pacer};

    /// The property the whole timer module exists for: a run that costs time
    /// does not push the following deadlines later. A naive
    /// `sleep(interval)` would put the second run at 60 ms, not 50 ms.
    #[test]
    fn deadlines_do_not_accumulate_the_cost_of_a_run() {
        let start = Instant::now();
        let mut pacer = Pacer::new(25, start);

        let first = pacer.next_deadline(start);
        // The first run cost 10 ms, so the second deadline is asked for at
        // 35 ms; it is still 50 ms after the start, not 60 ms.
        let second = pacer.next_deadline(start + Duration::from_millis(35));

        assert_eq!(first.duration_since(start), Duration::from_millis(25));
        assert_eq!(second.duration_since(start), Duration::from_millis(50));
    }

    /// A cadence that fell behind resynchronises rather than firing a burst.
    #[test]
    fn a_deadline_in_the_past_resynchronises_to_now() {
        let start = Instant::now();
        let mut pacer = Pacer::new(25, start);
        let now = start + Duration::from_secs(10);

        assert_eq!(pacer.next_deadline(now), now);
    }

    /// Zero would be a busy loop, so it is clamped.
    #[test]
    fn a_zero_interval_is_clamped() {
        let start = Instant::now();
        let mut pacer = Pacer::new(0, start);

        assert_eq!(
            pacer.next_deadline(start).duration_since(start),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn a_signalled_cancel_never_sleeps() {
        let cancel = Cancel::default();
        cancel.signal();

        let before = Instant::now();
        let stopped = cancel.sleep_until(before + Duration::from_secs(30));

        assert!(stopped);
        assert!(before.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn an_unsignalled_cancel_sleeps_out_its_deadline() {
        let cancel = Cancel::default();

        let before = Instant::now();
        let stopped = cancel.sleep_until(before + Duration::from_millis(20));

        assert!(!stopped);
        assert!(before.elapsed() >= Duration::from_millis(20));
    }
}

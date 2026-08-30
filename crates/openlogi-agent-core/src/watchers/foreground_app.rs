//! Focused-window polling watcher.

use std::time::Duration;

use openlogi_core::app::FocusedWindow;
use tokio::sync::mpsc;

use super::poll::{self, Poll};

/// Channel item: the window the platform reports as focused right now, or
/// `None` when the reading is unknown — no window is frontmost, or the
/// platform cannot say. It is never "no window": the orchestrator's reconciler
/// treats `None` as unknown and keeps the last identifiable window (spec §6).
pub type FocusReading = Option<FocusedWindow>;

/// Report the focused window every `period` while no push source is
/// available, or every `period * 3` once one is — the poll then only exists
/// as a reconciliation safety net (spec §6), since the push source already
/// reports the current window on every change.
///
/// The consumer reconciles by level (spec §6): it compares the profile that
/// should be applied against the one that is, on every reading, so a missed
/// edge corrects itself a period later instead of waiting for the user to
/// alt-tab twice.
pub fn spawn(period: Duration) -> mpsc::UnboundedReceiver<FocusReading> {
    if !cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    )) {
        // No way to read the frontmost window, so per-game profiles never switch.
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
    Poll {
        name: "openlogi-focus-watcher",
        period,
        degrades: "per-game profiles are disabled",
    }
    .every_into(tx, openlogi_hook::focused_window);
    rx
}

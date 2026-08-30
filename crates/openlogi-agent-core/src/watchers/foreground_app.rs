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

/// Report the focused window every `period`, changed or not.
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
    Poll {
        name: "openlogi-focus-watcher",
        period,
        degrades: "per-game profiles are disabled",
    }
    .every_into(tx, openlogi_hook::focused_window);
    rx
}

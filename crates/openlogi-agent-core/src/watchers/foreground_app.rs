//! Focused-window polling watcher.

use std::thread;
use std::time::Duration;

use openlogi_core::app::FocusedWindow;
use tokio::sync::mpsc;
use tracing::warn;

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
///
/// This is called from async code (`Armed::run`), but finding or subscribing
/// a focus source blocks on compositor / D-Bus / X11 I/O — on Linux,
/// `openlogi_hook::watch_focus` alone can take up to the gnome-shell
/// backend's 5 s method timeout. That work therefore runs on a dedicated
/// one-shot `openlogi-focus-setup` thread rather than inline on whichever
/// tokio worker called `spawn`; that thread hands off to `Poll::every_into`,
/// which spawns its own recurring `openlogi-focus-watcher` thread, and then
/// exits. Two threads end up backing this watcher, one of them short-lived.
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
    let spawned = thread::Builder::new()
        .name("openlogi-focus-setup".into())
        .spawn(move || {
            let pushed = openlogi_hook::watch_focus({
                let tx = tx.clone();
                move |reading| {
                    let _ = tx.send(reading);
                }
            });
            // With a push source the poll is only the reconciliation safety
            // net (spec §6): a few seconds is enough to heal a dropped signal.
            let period = if pushed { period * 3 } else { period };
            Poll {
                name: "openlogi-focus-watcher",
                period,
                degrades: "per-game profiles are disabled",
            }
            .every_into(tx, openlogi_hook::focused_window);
        });
    if let Err(error) = spawned {
        warn!(
            error = %error,
            watcher = "openlogi-focus-setup",
            "could not spawn watcher — per-game profiles are disabled"
        );
    }
    rx
}

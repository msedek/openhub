//! Frontmost backend for GNOME Shell (Wayland and X11), via a small companion
//! GNOME Shell extension that exports the focused window's WM_CLASS over D-Bus.
//!
//! GNOME (Mutter) implements neither wlr-foreign-toplevel nor any portal for
//! the focused window, and `org.gnome.Shell.Eval` is disabled by default, so a
//! privileged GNOME Shell extension is the only way to read the focused window
//! on a GNOME Wayland session. The extension lives in `gnome-shell-extension/`
//! in this crate and must be installed and enabled for this backend to
//! activate. When it is absent, [`GnomeShellSource::connect`] fails and backend
//! selection falls through to the next candidate (XWayland via X11).
//!
//! The extension returns the WM_CLASS — not the `.desktop` id — so the
//! identifier matches the X11 backend's, keeping per-app profile keys
//! consistent across X11, XWayland, and GNOME Wayland sessions.
//!
//! Only the session-bus connection is held in the backend; a lightweight proxy
//! is built per poll (no extra D-Bus traffic beyond the method call itself).

use std::sync::Once;
use std::thread;
use std::time::Duration;

use tracing::debug;
use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::proxy;

use super::FrontmostSource;
use crate::{FocusedWindow, ForegroundApp};

/// Cap on every D-Bus call to the extension. Without it, a stalled GNOME Shell
/// would block the polling thread forever (the probe runs inside the
/// `FRONTMOST_SOURCE` initializer, so a stall there would block every thread
/// that touches it).
const METHOD_TIMEOUT: Duration = Duration::from_secs(5);

/// D-Bus proxy for the OpenLogi GNOME Shell extension. Only the blocking proxy
/// is generated (`gen_async = false`), matching the synchronous poll contract.
#[proxy(
    interface = "org.openlogi.Frontmost",
    default_service = "org.openlogi.Frontmost",
    default_path = "/org/openlogi/Frontmost",
    gen_async = false
)]
trait Frontmost {
    /// WM_CLASS of the focused window, or "" when nothing is focused.
    #[zbus(name = "GetFocusedWmClass")]
    fn get_focused_wm_class(&self) -> zbus::Result<String>;

    /// WM_CLASS, title and client pid of the focused window; empty strings and
    /// `0` when nothing is focused. Extension v2 only.
    #[zbus(name = "GetFocusedWindow")]
    fn get_focused_window(&self) -> zbus::Result<(String, String, u32)>;

    /// Emitted on every focus change and on a title change of the focused
    /// window. Same triple as `GetFocusedWindow`.
    #[zbus(signal)]
    fn focus_changed(&self, wm_class: String, title: String, pid: u32) -> zbus::Result<()>;
}

/// Map a `(wm_class, title, pid)` triple from the extension's
/// `GetFocusedWindow` (or a synthesized one, for the `GetFocusedWmClass`
/// fallback) into a [`FocusedWindow`]. An empty `wm_class` means nothing is
/// focused.
fn describe(wm_class: String, title: String, pid: u32) -> Option<FocusedWindow> {
    if wm_class.is_empty() {
        return None;
    }
    Some(FocusedWindow {
        app: ForegroundApp::unnamed(wm_class),
        title: (!title.is_empty()).then_some(title),
        pid: (pid != 0).then_some(pid),
        steam_app_id: None,
    })
}

/// Frontmost backend talking to the OpenLogi GNOME Shell extension over the
/// session bus.
struct GnomeShellSource {
    conn: Connection,
    /// Logs the "extension is v1" `debug!` at most once per connection, so a
    /// v1 extension doesn't spam a log line on every ~1 Hz poll.
    logged_v1_fallback: Once,
}

impl GnomeShellSource {
    fn connect() -> Option<Self> {
        let conn = session_connection()?;
        // Probe reachability: a successful call (even an empty result) means the
        // OpenLogi extension is installed and exporting the service. An error
        // means it is absent/disabled, so this backend must not be selected.
        let proxy = FrontmostProxy::new(&conn)
            .map_err(|e| debug!("gnome-shell: proxy build failed: {e}"))
            .ok()?;
        proxy
            .get_focused_wm_class()
            .map_err(|e| debug!("gnome-shell: OpenLogi extension not reachable: {e}"))
            .ok()?;
        Some(Self {
            conn,
            logged_v1_fallback: Once::new(),
        })
    }
}

/// Build a bare session-bus connection with [`METHOD_TIMEOUT`] applied, doing
/// no reachability probe. Shared by [`GnomeShellSource::connect`] (which
/// layers a probe on top) and [`GnomeShellSource::watch`], which needs its
/// own connection because the poll thread keeps using the first one.
fn session_connection() -> Option<Connection> {
    Builder::session()
        .map_err(|e| debug!("gnome-shell: no session bus: {e}"))
        .ok()?
        .method_timeout(METHOD_TIMEOUT)
        .build()
        .map_err(|e| debug!("gnome-shell: connection build failed: {e}"))
        .ok()
}

impl FrontmostSource for GnomeShellSource {
    fn frontmost_app_id(&self) -> Option<String> {
        let proxy = FrontmostProxy::new(&self.conn)
            .map_err(|e| debug!("gnome-shell: proxy build failed: {e}"))
            .ok()?;
        let wm_class = proxy
            .get_focused_wm_class()
            .map_err(|e| debug!("gnome-shell: poll failed (extension gone or bus down?): {e}"))
            .ok()?;
        (!wm_class.is_empty()).then_some(wm_class)
    }

    fn focused_window(&self) -> Option<FocusedWindow> {
        let proxy = FrontmostProxy::new(&self.conn)
            .map_err(|e| debug!("gnome-shell: proxy build failed: {e}"))
            .ok()?;
        match proxy.get_focused_window() {
            Ok((wm_class, title, pid)) => describe(wm_class, title, pid),
            Err(e) => {
                self.logged_v1_fallback.call_once(|| {
                    debug!(
                        "gnome-shell: GetFocusedWindow unavailable ({e}), extension is v1 — \
                         falling back to GetFocusedWmClass (no title/pid)"
                    );
                });
                self.frontmost_app_id()
                    .map(|id| FocusedWindow::app(ForegroundApp::unnamed(id)))
            }
        }
    }

    fn watch(&self, on_reading: Box<dyn Fn(Option<FocusedWindow>) + Send + Sync>) -> bool {
        // A second connection: the poll thread keeps using `self.conn`, and
        // zbus connections are not meant to be shared across threads that
        // each drive their own blocking read loop.
        let Some(conn) = session_connection() else {
            return false;
        };
        let proxy = match FrontmostProxy::new(&conn) {
            Ok(proxy) => proxy,
            Err(e) => {
                debug!("gnome-shell: push proxy build failed: {e}");
                return false;
            }
        };
        // `receive_focus_changed` only installs a D-Bus match rule and
        // succeeds whether or not the service ever emits it — including
        // against a v1 extension, which has no such signal at all. Probe the
        // v2-only `GetFocusedWindow` method on this connection first, so a
        // v1 extension is correctly reported as having no push source
        // instead of a match rule that silently never fires (which would
        // otherwise leave the caller polling three times slower for nothing).
        if let Err(e) = proxy.get_focused_window() {
            debug!("gnome-shell: {e}, extension is v1 — no push, polling only");
            return false;
        }
        let stream = match proxy.receive_focus_changed() {
            Ok(s) => s,
            Err(e) => {
                debug!("gnome-shell: signal subscription failed: {e}");
                return false;
            }
        };
        thread::Builder::new()
            .name("openlogi-focus-push".into())
            .spawn(move || {
                for signal in stream {
                    let Ok(args) = signal.args() else {
                        debug!("gnome-shell: FocusChanged signal args failed to deserialize");
                        continue;
                    };
                    on_reading(describe(
                        args.wm_class().clone(),
                        args.title().clone(),
                        *args.pid(),
                    ));
                }
                debug!("gnome-shell: signal stream ended");
            })
            .is_ok()
    }

    fn name(&self) -> &'static str {
        "gnome-shell"
    }
}

/// Candidate constructor registered in [`super::wayland_candidates`].
pub(super) fn candidate() -> Option<Box<dyn FrontmostSource>> {
    GnomeShellSource::connect().map(|s| Box::new(s) as Box<dyn FrontmostSource>)
}

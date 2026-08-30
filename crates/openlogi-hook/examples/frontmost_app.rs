//! Smoke-test for `focused_window()`.
//!
//! Polls the focused window once per second and prints everything the
//! platform's focus source can read: the application (identifier and display
//! name — the two halves per-app profiles are keyed and labelled by), plus
//! the title, pid, and Steam AppID where a source reports them. Switch
//! between windows (and, on Linux, run a Steam game) while it runs to verify
//! detection.
//!
//! Worth pointing at every platform that has a frontmost reader: the
//! identifier's shape differs on each (bundle id, `WM_CLASS`, xdg `app_id`,
//! executable path). On a Wayland session it is also how you find out whether
//! the session resolved to a usable backend at all — a `None` here is the
//! reason per-app profiles would silently never switch there.
//!
//! # Usage
//!
//! ```text
//! cargo run --example frontmost_app -p openlogi-hook
//! ```

fn main() {
    println!("Polling the focused window every second — switch windows to test.");
    loop {
        match openlogi_hook::focused_window() {
            Some(window) => println!("{window:?}"),
            None => println!("(none — no frontmost window, or no reader on this platform)"),
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

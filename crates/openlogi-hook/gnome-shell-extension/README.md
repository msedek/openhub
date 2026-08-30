# OpenLogi Frontmost Window — GNOME Shell extension

GNOME (Mutter) does not let ordinary clients see which window is focused on
Wayland, and it implements neither `wlr-foreign-toplevel` nor a focused-window
portal. This minimal extension bridges that gap: it exports the WM_CLASS, title and
client pid of the focused window over D-Bus so OpenLogi's `gnome-shell`
frontmost backend can drive per-app and per-game mouse-profile switching.

It reads only `global.display.focus_window`'s WM_CLASS, title and pid. No
window contents, no input, no UI.

## D-Bus surface

- name: `org.openlogi.Frontmost`
- path: `/org/openlogi/Frontmost`
- method: `GetFocusedWmClass() -> s` (empty string when nothing is focused)
- method: `GetFocusedWindow() -> (s s u)` = (wmClass, title, pid); the empty
  triple (`"", "", 0`) when nothing is focused. Since v2.
- signal: `FocusChanged(s wmClass, s title, u pid)` — the same triple as
  `GetFocusedWindow`, emitted on every focus change and on the focused
  window's title change. Since v2.

Since v2 the extension also reads window titles and client pids, and pushes
them as a `FocusChanged` signal instead of only answering polls; all of it
stays on this machine and feeds per-app and per-game profile rules only. v2 is
push + pull: OpenLogi's hook still polls `GetFocusedWindow` as a
reconciliation safety net, at a slower cadence, once the signal is subscribed.

## Install

```sh
UUID=openlogi-frontmost@openlogi.dev
DEST="$HOME/.local/share/gnome-shell/extensions/$UUID"
mkdir -p "$DEST"
cp metadata.json extension.js "$DEST"/
```

On Wayland the shell cannot be reloaded in place, so **log out and back in** to
let GNOME pick up the newly added extension, then enable it:

```sh
gnome-extensions enable "$UUID"
gnome-extensions info "$UUID"   # State should be ACTIVE
```

## Verify

```sh
# Introspect the service:
busctl --user introspect org.openlogi.Frontmost /org/openlogi/Frontmost

# Focus a window, then query it:
gdbus call --session \
  -d org.openlogi.Frontmost \
  -o /org/openlogi/Frontmost \
  -m org.openlogi.Frontmost.GetFocusedWmClass

# Or the v2 method, which also returns the title and pid:
busctl --user call org.openlogi.Frontmost /org/openlogi/Frontmost \
  org.openlogi.Frontmost GetFocusedWindow

# Watch the push signal while alt-tabbing between windows:
busctl --user monitor org.openlogi.Frontmost
```

If `gdbus call` prints the focused window's WM_CLASS, OpenLogi's GNOME backend
will pick it up automatically the next time the hook starts. Alt-tabbing
should print one `FocusChanged` per switch in the `busctl monitor` output.

## Notes

- The `shell-version` list in `metadata.json` covers GNOME 45–50. Newer GNOME
  releases may need an added entry; the API used here (`Gio.DBusExportedObject`,
  `global.display.focus_window`, `Meta.Window.get_wm_class`,
  `Meta.Window.get_title`, `Meta.Window.get_pid`) has been stable across these
  versions.
- The extension name/UUID and the D-Bus name (`org.openlogi.*`) are placeholders
  that should track the project's namespace; if they change, update the matching
  constants in `crates/openlogi-hook/src/linux/gnome_shell.rs`.

// OpenLogi Frontmost Window — GNOME Shell extension.
//
// Exports a tiny D-Bus service that returns the WM_CLASS, title and client
// pid of the currently focused window. OpenLogi's `gnome_shell` frontmost
// backend polls this to drive per-app mouse-profile switching on GNOME
// Wayland, where the focused window is otherwise not visible to ordinary
// clients.
//
// Since v2 it also reads the window title and pid: some launchers (Steam
// chief among them) hide the real game behind a generic WM_CLASS, so a
// per-game profile needs the title and the pid — which feeds a Steam AppID
// lookup — as a fallback. v2 also pushes a `FocusChanged` signal on every
// focus change and on the focused window's title change, so OpenLogi can
// react immediately instead of waiting for its next poll; the poll stays as
// a reconciliation safety net. It reads only `global.display.focus_window`'s
// WM_CLASS, title and pid — no window contents, no input. ESM module style;
// targets GNOME Shell 45+.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_NAME = 'org.openlogi.Frontmost';
const DBUS_PATH = '/org/openlogi/Frontmost';
const DBUS_INTERFACE = `
<node>
  <interface name="org.openlogi.Frontmost">
    <method name="GetFocusedWmClass">
      <arg type="s" direction="out" name="wmClass"/>
    </method>
    <method name="GetFocusedWindow">
      <arg type="s" direction="out" name="wmClass"/>
      <arg type="s" direction="out" name="title"/>
      <arg type="u" direction="out" name="pid"/>
    </method>
    <signal name="FocusChanged">
      <arg type="s" name="wmClass"/>
      <arg type="s" name="title"/>
      <arg type="u" name="pid"/>
    </signal>
  </interface>
</node>`;

export default class OpenLogiFrontmostExtension extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(DBUS_INTERFACE, this);
        this._dbus.export(Gio.DBus.session, DBUS_PATH);
        this._nameId = Gio.bus_own_name_on_connection(
            Gio.DBus.session,
            DBUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            null,
            null);
        this._focusId = global.display.connect('notify::focus-window',
            () => this._onFocusChanged());
        this._titledWindow = null;
        this._titleId = 0;
    }

    disable() {
        this._untrackTitle();
        if (this._focusId) {
            global.display.disconnect(this._focusId);
            this._focusId = 0;
        }
        if (this._nameId) {
            Gio.bus_unown_name(this._nameId);
            this._nameId = 0;
        }
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
    }

    // D-Bus method org.openlogi.Frontmost.GetFocusedWmClass.
    GetFocusedWmClass() {
        const win = global.display.focus_window;
        if (!win)
            return '';
        return win.get_wm_class() || '';
    }

    // D-Bus method org.openlogi.Frontmost.GetFocusedWindow.
    GetFocusedWindow() {
        return this._describe(global.display.focus_window);
    }

    // Focus moved: follow the new window's title too, since a Proton game
    // often renames its window after the launcher hands over, and a profile
    // may match on that title.
    _onFocusChanged() {
        this._untrackTitle();
        const win = global.display.focus_window;
        if (win) {
            this._titledWindow = win;
            this._titleId = win.connect('notify::title', () => this._emit(win));
        }
        this._emit(win);
    }

    _untrackTitle() {
        // Clear the fields before touching the window: Mutter disposes a
        // MetaWindow at unmanage, and GJS throws on any access to a disposed
        // object. If a stale `disconnect` below throws, the fields must
        // already be cleared so the next `_onFocusChanged` doesn't inherit a
        // disposed window and throw before ever reaching `_emit`.
        const win = this._titledWindow;
        const id = this._titleId;
        this._titledWindow = null;
        this._titleId = 0;
        if (win && id) {
            try {
                win.disconnect(id);
            } catch (e) {
                // The window was already disposed (unmanaged); nothing left
                // to disconnect.
            }
        }
    }

    _emit(win) {
        this._dbus.emit_signal('FocusChanged',
            new GLib.Variant('(ssu)', this._describe(win)));
    }

    // [wmClass, title, pid] for a Meta.Window, or the empty triple.
    _describe(win) {
        if (!win)
            return ['', '', 0];
        const pid = win.get_pid();
        return [win.get_wm_class() || '', win.get_title() || '', pid > 0 ? pid : 0];
    }
}

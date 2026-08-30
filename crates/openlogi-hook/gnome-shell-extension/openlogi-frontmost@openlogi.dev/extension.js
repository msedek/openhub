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
// lookup — as a fallback. It reads only `global.display.focus_window`'s
// WM_CLASS, title and pid — no window contents, no input. ESM module style;
// targets GNOME Shell 45+.

import Gio from 'gi://Gio';
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
    }

    disable() {
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

    // [wmClass, title, pid] for a Meta.Window, or the empty triple.
    _describe(win) {
        if (!win)
            return ['', '', 0];
        const pid = win.get_pid();
        return [win.get_wm_class() || '', win.get_title() || '', pid > 0 ? pid : 0];
    }
}

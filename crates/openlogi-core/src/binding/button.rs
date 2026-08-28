//! Rebindable mouse/keyboard button identifiers.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::Action;

/// One of the user-rebindable hotspots on a Logi mouse. The order matches the
/// physical layout from front to side; [`ButtonId::ALL`] is consumed by the
/// default-binding generator and the popover trigger list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ButtonId {
    /// The primary button. The OS hook sees it and may remap it, but it keeps
    /// a floor no other button has — see [`ButtonId::may_suppress`].
    LeftClick,
    /// The secondary button. Reaches the OS hook like [`ButtonId::LeftClick`],
    /// with no floor of its own: bind it and it is suppressed.
    RightClick,
    /// The wheel click — one of the buttons the OS hook remaps.
    MiddleClick,
    /// The thumb-side "back" button (mouse button 4), remapped by the OS hook.
    Back,
    /// The thumb-side "forward" button (mouse button 5), remapped by the OS hook.
    Forward,
    /// The "ModeShift" button under the wheel — typically used for SmartShift /
    /// DPI cycle. Named `DpiToggle` for historical reasons. On a gaming mouse
    /// this is `BTN_TASK`, which the OS hook sees like any other button.
    DpiToggle,
    /// The horizontal thumb wheel's click. Kept in [`ButtonId::ALL`] so its
    /// default still seeds and dispatches when the wheel is diverted, even
    /// though the mouse model surfaces one paired rotation control instead of
    /// the click (see `mouse_model::geometry`).
    Thumbwheel,
    /// Rotating the thumb wheel "up" (positive rotation). Bound, by default, to
    /// continuous horizontal scroll; see the agent-core `watchers`-side dispatch.
    ThumbwheelScrollUp,
    /// Rotating the thumb wheel "down" (negative rotation).
    ThumbwheelScrollDown,
    /// The HID++ gesture button on MX-line devices. The press itself
    /// fires the bound action; swipe directions are P1.5 territory.
    GestureButton,
    /// Keyboard F-row "Search" control (`0x1b04` CID `0x00d4`,
    /// `MultiPlatform_Search`) — F4 on the Signature series.
    KeySearch,
    /// Keyboard "Dictation" control (CID `0x0103`) — F5 on the Signature series.
    KeyDictation,
    /// Keyboard "Emoji" control (CID `0x0108`) — F6 on the Signature series.
    KeyEmoji,
    /// Keyboard "Screen Capture" control (CID `0x010a`) — F7 on the Signature
    /// series.
    KeyScreenCapture,
    /// Keyboard "Mute Microphone" control (CID `0x011c`) — F8 on the Signature
    /// series.
    KeyMicMute,
    /// Keyboard "Play/Pause" control (CID `0x00e5`) — F9 on the Signature series.
    KeyPlayPause,
    /// Keyboard "Mute" control (CID `0x00e7`) — F10 on the Signature series.
    KeyMute,
    /// Keyboard "Volume Down" control (CID `0x00e8`) — F11 on the Signature
    /// series.
    KeyVolumeDown,
    /// Keyboard "Volume Up" control (CID `0x00e9`) — F12 on the Signature
    /// series.
    KeyVolumeUp,
    /// The MX Master 4 Haptic Sense Panel — the touch-sensitive thumb rest
    /// (Logi metadata slot `ASSIGNMENT_NAME_SHOW_RADIAL_MENU`, HID++ CID
    /// `0x01a0`). A separate physical control from [`ButtonId::GestureButton`];
    /// captured over HID++ like it, and eligible as the gesture owner.
    HapticPanel,
    /// Tilting the main wheel left — `0x1b04` CID `0x005b` ("Left Scroll"),
    /// Logi metadata slot `SLOT_NAME_LEFT_SCROLL_BUTTON`. A distinct control
    /// from the thumb wheel: it is a plain divertable button, not a rotation,
    /// and it lives on the main wheel of mice like the MX Anywhere 2S.
    WheelTiltLeft,
    /// Tilting the main wheel right — `0x1b04` CID `0x005d` ("Right Scroll"),
    /// Logi metadata slot `SLOT_NAME_RIGHT_SCROLL_BUTTON`. Counterpart to
    /// [`ButtonId::WheelTiltLeft`].
    ///
    /// Declared last: the TOML config and any serialized form encode the
    /// variant identifier / index, so new buttons are append-only.
    WheelTiltRight,
}

impl ButtonId {
    /// Every rebindable button in declaration (physical front-to-side) order —
    /// the iteration source for default-binding seeding and the popover
    /// trigger list.
    pub const ALL: [ButtonId; 13] = [
        ButtonId::LeftClick,
        ButtonId::RightClick,
        ButtonId::MiddleClick,
        ButtonId::WheelTiltLeft,
        ButtonId::WheelTiltRight,
        ButtonId::Back,
        ButtonId::Forward,
        ButtonId::DpiToggle,
        ButtonId::Thumbwheel,
        ButtonId::ThumbwheelScrollUp,
        ButtonId::ThumbwheelScrollDown,
        ButtonId::GestureButton,
        ButtonId::HapticPanel,
    ];

    /// The divertable keyboard F-row controls, in F-row order. Kept out of
    /// [`ButtonId::ALL`]: that array seeds mouse defaults and the mouse
    /// popover trigger list, while keyboard keys stay native unless the user
    /// binds them (an unbound key is never diverted).
    pub const KEYBOARD_KEYS: [ButtonId; 9] = [
        ButtonId::KeySearch,
        ButtonId::KeyDictation,
        ButtonId::KeyEmoji,
        ButtonId::KeyScreenCapture,
        ButtonId::KeyMicMute,
        ButtonId::KeyPlayPause,
        ButtonId::KeyMute,
        ButtonId::KeyVolumeDown,
        ButtonId::KeyVolumeUp,
    ];

    /// Whether the OS hook (macOS `CGEventTap` / Linux evdev) sees this button
    /// at all, and may therefore remap it.
    ///
    /// This is the set the platform hooks actually decode from the raw event
    /// stream: `BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`, `BTN_SIDE`, `BTN_EXTRA`
    /// and `BTN_TASK` on Linux, and their equivalents elsewhere. The thumb
    /// wheel, the wheel tilts and the dedicated gesture controls are absent
    /// because they never reach the hook — they arrive over HID++ diversion.
    ///
    /// Seeing a button is not the same as being allowed to swallow it: that is
    /// [`ButtonId::may_suppress`], which adds the primary click's floor.
    #[must_use]
    pub fn is_os_hook_button(self) -> bool {
        matches!(
            self,
            ButtonId::LeftClick
                | ButtonId::RightClick
                | ButtonId::MiddleClick
                | ButtonId::Back
                | ButtonId::Forward
                | ButtonId::DpiToggle
        )
    }

    /// Whether this button may host an OS-hook hold-and-swipe gesture: the
    /// wheel click and the two thumb buttons.
    ///
    /// A subset of [`ButtonId::is_os_hook_button`], and deliberately narrower
    /// than it. Holding a button is the gesture's begin edge, so a control the
    /// user holds for something else — dragging with the primary click, opening
    /// a context menu with the secondary — cannot also be a swipe source
    /// without breaking the thing it is already for.
    #[must_use]
    pub fn is_os_hook_gesture_button(self) -> bool {
        matches!(
            self,
            ButtonId::MiddleClick | ButtonId::Back | ButtonId::Forward
        )
    }

    /// Whether the OS hook may swallow this button's physical event, given the
    /// action bound to it.
    ///
    /// Every button the hook sees is suppressible once it carries a binding —
    /// that is what lets the button behind the wheel host a remap and the
    /// secondary button become an auto-clicker. The primary click is the one
    /// exception, and it is a safety floor rather than a preference: it is the
    /// user's only route to the GUI that would undo the profile. A binding of
    /// [`Action::None`] synthesises nothing at all, so suppressing the left
    /// button for it would leave the desktop unclickable with no way back.
    /// The button therefore keeps working unless the user explicitly put
    /// something else in its place.
    #[must_use]
    pub fn may_suppress(self, action: &Action) -> bool {
        if !self.is_os_hook_button() {
            return false;
        }
        if matches!(self, ButtonId::LeftClick) {
            return !matches!(action, Action::None);
        }
        true
    }

    /// Whether this button is a HID++ gesture source — a control that is
    /// captured over HID++ raw-XY diversion (never the OS hook) and can
    /// therefore own the gesture role with swipe directions: the dedicated
    /// gesture button, or the MX Master 4 haptic panel. The capture layer maps
    /// each to its control ID.
    #[must_use]
    pub fn is_hidpp_gesture_source(self) -> bool {
        matches!(self, ButtonId::GestureButton | ButtonId::HapticPanel)
    }

    /// Human-readable label for popovers and tooltips.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ButtonId::LeftClick => "Left Click",
            ButtonId::RightClick => "Right Click",
            ButtonId::MiddleClick => "Middle Click",
            ButtonId::WheelTiltLeft => "Tilt Left",
            ButtonId::WheelTiltRight => "Tilt Right",
            ButtonId::Back => "Back",
            ButtonId::Forward => "Forward",
            ButtonId::DpiToggle => "DPI Toggle",
            ButtonId::Thumbwheel => "Thumb Wheel",
            ButtonId::ThumbwheelScrollUp => "Thumb Wheel Up",
            ButtonId::ThumbwheelScrollDown => "Thumb Wheel Down",
            ButtonId::GestureButton => "Gesture Button",
            ButtonId::KeySearch => "Search Key",
            ButtonId::KeyDictation => "Dictation Key",
            ButtonId::KeyEmoji => "Emoji Key",
            ButtonId::KeyScreenCapture => "Screen Capture Key",
            ButtonId::KeyMicMute => "Mic Mute Key",
            ButtonId::KeyPlayPause => "Play/Pause Key",
            ButtonId::KeyMute => "Mute Key",
            ButtonId::KeyVolumeDown => "Volume Down Key",
            ButtonId::KeyVolumeUp => "Volume Up Key",
            ButtonId::HapticPanel => "Haptic Panel",
        }
    }
}

impl fmt::Display for ButtonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

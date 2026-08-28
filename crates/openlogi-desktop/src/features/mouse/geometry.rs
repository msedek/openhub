//! Geometry helpers for the centre mouse model.
//!
//! These functions keep Logitech asset coordinate translation and fallback
//! label layout separate from the GPUI element tree in `view`.

use openlogi_core::binding::ButtonId;

use super::hotspots::{Hotspot, MOUSE_MODEL_SIZE, MouseControlId};
use super::leader_lines::{Label, Side};
use crate::services::assets::ResolvedAsset;

/// Approx pixel width of each hotspot hit-target on a legacy depot asset.
/// Logitech only gives us a marker point per button, not a rectangle, so we
/// size by hand.
const ASSET_HOTSPOT: f32 = 56.;

/// Floor for a hotspot drawn from a device drawing. The drawn rectangle is
/// used as-is when it is big enough; the thin side buttons on most mice are
/// only a dozen units wide, which is an unfair click target.
const MIN_HOTSPOT: f32 = 30.;

/// Which physical control each `buttonN` of a device drawing is.
///
/// The drawings number their buttons the way libratbag's driver enumerates the
/// device, which for a Logitech mouse is the order its `0x1b04` control IDs
/// come back in: left click, right click, wheel click, then the rear and front
/// thumb buttons. Those five hold across every Logitech drawing in the set. The
/// sixth is where a fixed table starts guessing — it is the DPI button under
/// the wheel on a G703 (verified against the drawing), the sniper button on a
/// G502, and the thumb wheel on an MX Master — and the seventh onward have no
/// [`ButtonId`] to carry them at all, so they are measured and embedded but not
/// surfaced.
///
/// The rectangle is right either way; only the name can be. The real fix is not
/// a longer table: the agent already walks `0x1b04` on every device, and that
/// walk returns the controls in exactly this order, so a device's own control
/// list can replace this constant and make `buttonN` an exact key.
const BUTTON_ORDER: [ButtonId; 6] = [
    ButtonId::LeftClick,
    ButtonId::RightClick,
    ButtonId::MiddleClick,
    ButtonId::Back,
    ButtonId::Forward,
    ButtonId::DpiToggle,
];

/// Height of a side-label card. The layout needs it to group related cards
/// without allowing them to overlap at the minimum model height.
pub(super) const LABEL_H: f32 = 56.;

/// Empty space between the grouped Back and Forward cards when the viewport
/// has enough room to pull them closer than the regular even spacing.
const NAVIGATION_GROUP_GAP: f32 = 16.;

/// Whether label cards occupy one or both sides of the device render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelDistribution {
    LeftOnly,
    BothSides,
}

/// Scale the device image to *fit inside* a `max_w` × `target_h` box while
/// preserving the **actual PNG's** aspect ratio. A tall device (a mouse) is
/// bound by the height; a wide one (a keyboard) is bound by the width — which
/// is what stops a wide keyboard render from overflowing the panel (#272).
///
/// The metadata's `origin` reports the silhouette bbox inside the PNG, which
/// is typically narrower than the full image (Logi pads transparent strips on
/// both sides); sizing by origin causes `ObjectFit::Contain` to letterbox
/// vertically and pulls every hotspot off the rendered button.
#[expect(
    clippy::cast_precision_loss,
    reason = "device images are < 4096 px on either axis — well within f32 mantissa"
)]
pub fn asset_dimensions_for_png(asset: &ResolvedAsset, target_h: f32, max_w: f32) -> (f32, f32) {
    if let Some(art) = asset.art {
        let aspect = art.canvas.width / art.canvas.height;
        let w = target_h * aspect;
        return if w > max_w {
            (max_w, max_w / aspect)
        } else {
            (w, target_h)
        };
    }
    if asset.png_height == 0 {
        return MOUSE_MODEL_SIZE;
    }
    let aspect = (asset.png_width as f32) / (asset.png_height as f32);
    let w = target_h * aspect;
    if w > max_w {
        (max_w, max_w / aspect)
    } else {
        (w, target_h)
    }
}

/// Whether the asset exposes any remappable button markers. Mice do (so the
/// model reserves a side gutter for their leader-line labels); keyboards and
/// other label-less devices don't, so the model can hand them the full width.
pub fn asset_has_button_labels(asset: &ResolvedAsset) -> bool {
    if let Some(art) = asset.art {
        return art
            .buttons
            .iter()
            .any(|button| control_for(button.index).is_some());
    }
    asset
        .metadata
        .assignments()
        .any(|a| map_slot_name(&a.slot_name).is_some())
}

/// Convert authored marker points into mouse-local pixel rects.
///
/// OpenHub geometry uses absolute coordinates in the generated art's canvas.
/// The compatibility path below retains the inherited percent/origin mapping
/// for isolated legacy tests.
///
/// Logi's markers are percentages of `origin` (the silhouette bbox).
/// Within the actual PNG, that bbox is centred with equal padding on the
/// left and right. We render at the *PNG's* full aspect (no letterboxing)
/// so the marker translation is:
///
/// ```text
/// bbox_w_rendered = mouse_w * origin.width  / png.width
/// bbox_x_offset   = (mouse_w - bbox_w_rendered) / 2
/// hotspot.x       = bbox_x_offset + marker.x / 100 * bbox_w_rendered
/// hotspot.y       = marker.y / 100 * mouse_h     // height ratio is 1:1
/// ```
///
/// Primary left/right clicks deliberately have no entry — Logi never
/// exposes them as remappable (and Options+ doesn't either), so we don't
/// invent markers for them.
#[expect(
    clippy::cast_precision_loss,
    reason = "device images are < 4096 px on either axis — well within f32 mantissa"
)]
pub fn asset_hotspots_for_png(asset: &ResolvedAsset, mouse_w: f32, mouse_h: f32) -> Vec<Hotspot> {
    if let Some(art) = asset.art {
        let scale_x = mouse_w / art.canvas.width;
        let scale_y = mouse_h / art.canvas.height;
        return art
            .buttons
            .iter()
            .filter_map(|button| {
                let id = control_for(button.index)?;
                let (cx, cy) = button.center();
                let w = (button.width * scale_x).max(MIN_HOTSPOT);
                let h = (button.height * scale_y).max(MIN_HOTSPOT);
                Some(Hotspot {
                    id,
                    x: cx * scale_x - w / 2.,
                    y: cy * scale_y - h / 2.,
                    w,
                    h,
                })
            })
            .collect();
    }

    let png_w = asset.png_width as f32;
    let origin_w = asset
        .metadata
        .origin()
        .map_or(png_w, |o| o.width as f32)
        .min(png_w);
    let bbox_w_rendered = if png_w > 0. {
        mouse_w * origin_w / png_w
    } else {
        mouse_w
    };
    let bbox_x_offset = (mouse_w - bbox_w_rendered) / 2.;
    let marker_to_canvas = |mx: f32, my: f32| -> (f32, f32) {
        let cx = bbox_x_offset + mx / 100. * bbox_w_rendered;
        let cy = my / 100. * mouse_h;
        (cx, cy)
    };

    let hotspots: Vec<Hotspot> = asset
        .metadata
        .assignments()
        .filter_map(|a| {
            let id = map_slot_name(&a.slot_name)?;
            let (cx, cy) = marker_to_canvas(a.marker.x, a.marker.y);
            Some(Hotspot {
                id,
                x: cx - ASSET_HOTSPOT / 2.,
                y: cy - ASSET_HOTSPOT / 2.,
                w: ASSET_HOTSPOT,
                h: ASSET_HOTSPOT,
            })
        })
        .collect();

    hotspots
}

/// Lay labels out evenly down one or both sides of the mouse. A two-sided
/// layout sends the leftmost half of the hotspots left and the rightmost half
/// right, then orders each side by hotspot height. Back and Forward stay
/// adjacent when both are on the same side because they form one navigation
/// pair, even when another marker sits between them.
pub fn labels_from_hotspots(
    hotspots: &[Hotspot],
    mouse_h: f32,
    distribution: LabelDistribution,
) -> Vec<Label> {
    if hotspots.is_empty() {
        return Vec::new();
    }

    let mut labels: Vec<Label> = hotspots
        .iter()
        .map(|hotspot| Label {
            id: hotspot.id,
            side: Side::Left,
            y: 0.,
        })
        .collect();
    if distribution == LabelDistribution::BothSides {
        let mut horizontal_order: Vec<usize> = (0..hotspots.len()).collect();
        horizontal_order
            .sort_by(|&a, &b| hotspots[a].center().0.total_cmp(&hotspots[b].center().0));
        for index in horizontal_order
            .into_iter()
            .skip(hotspots.len().div_ceil(2))
        {
            labels[index].side = Side::Right;
        }
    }

    position_labels(hotspots, mouse_h, &mut labels);
    labels
}

#[expect(
    clippy::cast_precision_loss,
    reason = "hotspot count is bounded by ButtonId variants — well under f32 mantissa"
)]
fn position_labels(hotspots: &[Hotspot], mouse_h: f32, labels: &mut [Label]) {
    for side in [Side::Left, Side::Right] {
        let mut vertical_order: Vec<usize> = labels
            .iter()
            .enumerate()
            .filter_map(|(index, label)| (label.side == side).then_some(index))
            .collect();
        vertical_order.sort_by(|&a, &b| hotspots[a].center().1.total_cmp(&hotspots[b].center().1));
        let back = vertical_order
            .iter()
            .position(|&index| labels[index].id == ButtonId::Back.into());
        let forward = vertical_order
            .iter()
            .position(|&index| labels[index].id == ButtonId::Forward.into());
        let navigation_pair = if let (Some(back), Some(forward)) = (back, forward) {
            let first = back.min(forward);
            let second = back.max(forward);
            if second > first + 1 {
                let navigation_button = vertical_order.remove(second);
                vertical_order.insert(first + 1, navigation_button);
            }
            Some((vertical_order[first], vertical_order[first + 1]))
        } else {
            None
        };
        let step = mouse_h / (vertical_order.len() as f32 + 1.);
        for (slot, index) in vertical_order.into_iter().enumerate() {
            labels[index].y = step * (slot as f32 + 1.);
        }
        if let Some((first, second)) = navigation_pair {
            let grouped_step = step.min(LABEL_H + NAVIGATION_GROUP_GAP);
            let adjustment = (step - grouped_step) / 2.;
            labels[first].y += adjustment;
            labels[second].y -= adjustment;
        }
    }
}

/// Label positions for the synthetic fallback silhouette.
pub fn default_labels(thumbwheel: bool, distribution: LabelDistribution) -> Vec<Label> {
    labels_from_hotspots(
        &super::hotspots::default_hotspots(thumbwheel),
        MOUSE_MODEL_SIZE.1,
        distribution,
    )
}

/// Logitech's stable slot vocabulary → OpenLogi's visual control IDs. Intentionally
/// conservative; unknown names fall through so widening `MouseControlId` later
/// doesn't break old depots.
fn map_slot_name(name: &str) -> Option<MouseControlId> {
    match name {
        "SLOT_NAME_LEFT_BUTTON" => Some(MouseControlId::Button(ButtonId::LeftClick)),
        "SLOT_NAME_RIGHT_BUTTON" => Some(MouseControlId::Button(ButtonId::RightClick)),
        "SLOT_NAME_MIDDLE_BUTTON" => Some(MouseControlId::Button(ButtonId::MiddleClick)),
        // The main wheel's tilt. Logi names the two slots after the scroll they
        // produce in firmware; each is its own reprogrammable control
        // (`0x1b04` CIDs `0x005b` / `0x005d`), not part of the middle click.
        "SLOT_NAME_LEFT_SCROLL_BUTTON" | "SLOT_NAME_SCROLL_LEFT" => {
            Some(MouseControlId::Button(ButtonId::WheelTiltLeft))
        }
        "SLOT_NAME_RIGHT_SCROLL_BUTTON" | "SLOT_NAME_SCROLL_RIGHT" => {
            Some(MouseControlId::Button(ButtonId::WheelTiltRight))
        }
        "SLOT_NAME_BACK_BUTTON" => Some(MouseControlId::Button(ButtonId::Back)),
        "SLOT_NAME_FORWARD_BUTTON" => Some(MouseControlId::Button(ButtonId::Forward)),
        "SLOT_NAME_MODESHIFT_BUTTON" | "SLOT_NAME_DPI_BUTTON" => {
            Some(MouseControlId::Button(ButtonId::DpiToggle))
        }
        "SLOT_NAME_THUMBWHEEL" => Some(MouseControlId::ThumbwheelRotation),
        "SLOT_NAME_GESTURE_BUTTON" => Some(MouseControlId::Button(ButtonId::GestureButton)),
        // The MX Master 4 Haptic Sense Panel. Logi names the slot after its
        // Options+ default assignment (the radial Actions Ring menu), but the
        // marker is the panel itself.
        "ASSIGNMENT_NAME_SHOW_RADIAL_MENU" => Some(MouseControlId::Button(ButtonId::HapticPanel)),
        _ => None,
    }
}

/// The control a drawing's `buttonN` stands for, or `None` when the index is
/// past what [`BUTTON_ORDER`] can name.
fn control_for(index: u32) -> Option<MouseControlId> {
    let index = usize::try_from(index).ok()?;
    BUTTON_ORDER.get(index).map(|button| (*button).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::mouse::hotspots::default_hotspots;
    use crate::services::assets::AssetResolver;
    use openlogi_core::device::{DeviceModelInfo, DeviceTransports};

    fn g703_asset() -> ResolvedAsset {
        let model = DeviceModelInfo {
            entity_count: 0,
            serial_number: None,
            unit_id: [0; 4],
            transports: DeviceTransports::default(),
            model_ids: [0x4086, 0, 0],
            extended_model_id: 0,
        };
        AssetResolver::new()
            .resolve(&model, Some("G703 LIGHTSPEED HERO"))
            .expect("G703 local art")
    }

    #[test]
    fn default_labels_include_capability_gated_thumbwheel() {
        assert!(
            !default_labels(false, LabelDistribution::LeftOnly)
                .iter()
                .any(|label| label.id == MouseControlId::ThumbwheelRotation)
        );
        assert_eq!(
            default_labels(true, LabelDistribution::LeftOnly)
                .iter()
                .filter(|label| label.id == MouseControlId::ThumbwheelRotation)
                .count(),
            1
        );
    }

    #[test]
    fn thumbwheel_metadata_maps_to_one_rotation_control() {
        assert_eq!(
            map_slot_name("SLOT_NAME_THUMBWHEEL"),
            Some(MouseControlId::ThumbwheelRotation)
        );
    }

    #[test]
    fn dpi_slot_names_map_to_dpi_toggle_button() {
        assert_eq!(
            map_slot_name("SLOT_NAME_MODESHIFT_BUTTON"),
            Some(MouseControlId::Button(ButtonId::DpiToggle))
        );
        assert_eq!(
            map_slot_name("SLOT_NAME_DPI_BUTTON"),
            Some(MouseControlId::Button(ButtonId::DpiToggle))
        );
    }

    #[test]
    fn wheel_tilt_slot_names_map_to_their_own_controls() {
        // MX Anywhere uses the longer names; MX Ergo uses the shorter aliases.
        for name in ["SLOT_NAME_LEFT_SCROLL_BUTTON", "SLOT_NAME_SCROLL_LEFT"] {
            assert_eq!(
                map_slot_name(name),
                Some(MouseControlId::Button(ButtonId::WheelTiltLeft))
            );
        }
        for name in ["SLOT_NAME_RIGHT_SCROLL_BUTTON", "SLOT_NAME_SCROLL_RIGHT"] {
            assert_eq!(
                map_slot_name(name),
                Some(MouseControlId::Button(ButtonId::WheelTiltRight))
            );
        }
    }

    #[test]
    fn labels_track_hotspots_and_avoid_crossing() {
        let hotspots = default_hotspots(true);
        let labels =
            labels_from_hotspots(&hotspots, MOUSE_MODEL_SIZE.1, LabelDistribution::LeftOnly);
        assert_eq!(labels.len(), hotspots.len());

        let mut ys: Vec<f32> = labels.iter().map(|l| l.y).collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        assert_eq!(ys.len(), labels.len(), "each label gets a distinct slot");
    }

    #[test]
    fn navigation_labels_stay_together_when_haptic_marker_sits_between() {
        let hotspots = [
            Hotspot {
                id: ButtonId::Forward.into(),
                x: 0.,
                y: 100.,
                w: 10.,
                h: 10.,
            },
            Hotspot {
                id: ButtonId::HapticPanel.into(),
                x: 0.,
                y: 200.,
                w: 10.,
                h: 10.,
            },
            Hotspot {
                id: ButtonId::Back.into(),
                x: 0.,
                y: 300.,
                w: 10.,
                h: 10.,
            },
        ];

        let mut labels =
            labels_from_hotspots(&hotspots, MOUSE_MODEL_SIZE.1, LabelDistribution::LeftOnly);
        labels.sort_by(|a, b| a.y.total_cmp(&b.y));

        assert_eq!(
            labels.iter().map(|label| label.id).collect::<Vec<_>>(),
            [
                MouseControlId::Button(ButtonId::Forward),
                MouseControlId::Button(ButtonId::Back),
                MouseControlId::Button(ButtonId::HapticPanel),
            ]
        );
        let navigation_gap = labels[1].y - labels[0].y;
        let haptic_gap = labels[2].y - labels[1].y;
        assert!(navigation_gap < haptic_gap);
        assert!(navigation_gap >= LABEL_H);
    }

    #[test]
    fn a_two_sided_layout_uses_both_sides() {
        let hotspots = default_hotspots(true);
        let labels =
            labels_from_hotspots(&hotspots, MOUSE_MODEL_SIZE.1, LabelDistribution::BothSides);

        assert!(labels.iter().any(|label| label.side == Side::Left));
        assert!(labels.iter().any(|label| label.side == Side::Right));
    }

    /// The drawing numbers its buttons; this asserts the numbering lands on
    /// the physical controls it is supposed to. Left click on the left of the
    /// top half, right click opposite it, the wheel between them, the two
    /// thumb buttons down the left flank with the forward one ahead of the
    /// back one, and DPI behind the wheel on the same centre line.
    #[test]
    fn g703_hotspots_land_on_the_drawn_buttons() {
        let asset = g703_asset();
        let (width, height) = (160., 280.);
        let hotspots = asset_hotspots_for_png(&asset, width, height);
        assert_eq!(hotspots.len(), 6, "the G703 drawing names six buttons");

        let centre = |button: ButtonId| {
            hotspots
                .iter()
                .find(|hotspot| hotspot.id == MouseControlId::Button(button))
                .expect("every mapped button has a hotspot")
                .center()
        };
        let left = centre(ButtonId::LeftClick);
        let right = centre(ButtonId::RightClick);
        let wheel = centre(ButtonId::MiddleClick);
        let back = centre(ButtonId::Back);
        let forward = centre(ButtonId::Forward);
        let dpi = centre(ButtonId::DpiToggle);

        assert!(
            left.0 < wheel.0 && wheel.0 < right.0,
            "wheel between the clicks"
        );
        assert!(
            left.1 < height / 2. && right.1 < height / 2.,
            "clicks up front"
        );
        assert!(
            back.0 < width / 3. && forward.0 < width / 3.,
            "both thumb buttons sit on the left flank"
        );
        assert!(forward.1 < back.1, "the forward button is the front one");
        assert!(dpi.1 > wheel.1, "DPI sits behind the wheel");
        assert!(
            (dpi.0 - wheel.0).abs() < width * 0.1,
            "and on the wheel's centre line"
        );
    }

    /// Hotspots are the drawn rectangles scaled into the rendered image, so a
    /// button's centre must track its element's centre exactly.
    #[test]
    fn hotspots_scale_from_the_drawing_canvas() {
        let asset = g703_asset();
        let art = asset.art.expect("the G703 resolves to a drawing");
        let (width, height) = (160., 280.);
        let hotspots = asset_hotspots_for_png(&asset, width, height);

        for button in art.buttons {
            let Some(id) = control_for(button.index) else {
                continue;
            };
            let hotspot = hotspots
                .iter()
                .find(|hotspot| hotspot.id == id)
                .expect("a mapped button is a hotspot");
            let (drawn_x, drawn_y) = button.center();
            let (x, y) = hotspot.center();
            assert!(
                (x - drawn_x * width / art.canvas.width).abs() < 0.01,
                "button {} drifted horizontally: {x}",
                button.index
            );
            assert!(
                (y - drawn_y * height / art.canvas.height).abs() < 0.01,
                "button {} drifted vertically: {y}",
                button.index
            );
        }
    }

    /// A mouse's thumb buttons are drawn as slivers a dozen units wide. They
    /// still have to be clickable.
    #[test]
    fn every_hotspot_is_big_enough_to_click() {
        let hotspots = asset_hotspots_for_png(&g703_asset(), 160., 280.);
        for hotspot in &hotspots {
            assert!(
                hotspot.w >= MIN_HOTSPOT && hotspot.h >= MIN_HOTSPOT,
                "{:?} is only {}x{}",
                hotspot.id,
                hotspot.w,
                hotspot.h
            );
        }
    }

    /// Piper draws every G703 callout into the right-hand gutter, which would
    /// stack all six cards on one side and leave the other empty. Sides come
    /// from the hotspots' own positions instead: the left flank goes left, the
    /// wheel side goes right.
    #[test]
    fn g703_labels_fill_both_gutters() {
        let asset = g703_asset();
        let hotspots = asset_hotspots_for_png(&asset, 160., 280.);
        let labels = labels_from_hotspots(&hotspots, 280., LabelDistribution::BothSides);

        for (button, expected_side) in [
            (ButtonId::LeftClick, Side::Left),
            (ButtonId::Back, Side::Left),
            (ButtonId::Forward, Side::Left),
            (ButtonId::RightClick, Side::Right),
            (ButtonId::MiddleClick, Side::Right),
            (ButtonId::DpiToggle, Side::Right),
        ] {
            let label = labels
                .iter()
                .find(|label| label.id == MouseControlId::Button(button))
                .expect("every G703 button has a label");
            assert_eq!(label.side, expected_side, "wrong side for {button:?}");
        }
    }

    /// The drawing's lit zones stay addressable — the lighting UI tints them.
    #[test]
    fn the_g703_keeps_its_two_lit_zones() {
        let art = g703_asset().art.expect("the G703 resolves to a drawing");
        assert_eq!(art.leds.len(), 2);
        assert!(art.leds.iter().all(|led| led.width > 0. && led.height > 0.));
    }
}

#!/usr/bin/env python3
"""Generate original, offline OpenHub device illustrations.

The Python geometry in this module is canonical.  Generic emitters consume the
same immutable shape data to write human-inspectable SVG and antialiased Pillow
rasters; SVG is never parsed or used as a rendering input.
"""

from __future__ import annotations

import hashlib
import json
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path
from typing import TypeAlias

from PIL import Image, ImageColor, ImageDraw


SUPERSAMPLE = 4
RASTER_HEIGHTS = (120, 320)
RESAMPLE = Image.Resampling.LANCZOS

NIGHT = "#08111F"
DEEP_GRAPHITE = "#0D1828"
GRAPHITE = "#12233A"
RAISED_GRAPHITE = "#19304B"
EDGE = "#7390A6"
MINERAL = "#F2F7FA"
BLUE = "#2563EB"
CYAN = "#22D3C5"


@dataclass(frozen=True)
class Point:
    """One point in a device's canonical canvas."""

    x: float
    y: float


@dataclass(frozen=True)
class Cubic:
    """A cubic Bézier segment ending at ``end``."""

    control_1: Point
    control_2: Point
    end: Point


@dataclass(frozen=True)
class PathShape:
    """A closed or open path built from cubic Bézier segments."""

    element_id: str
    start: Point
    segments: tuple[Cubic, ...]
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float = 0.0
    closed: bool = True
    opacity: float = 1.0


@dataclass(frozen=True)
class EllipseShape:
    """An ellipse described by its bounding box."""

    element_id: str
    box: tuple[float, float, float, float]
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float = 0.0
    opacity: float = 1.0


@dataclass(frozen=True)
class RoundedRectShape:
    """A rounded rectangle in canonical canvas units."""

    element_id: str
    box: tuple[float, float, float, float]
    radius: float
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float = 0.0
    opacity: float = 1.0


@dataclass(frozen=True)
class LineShape:
    """A stroked polyline."""

    element_id: str
    points: tuple[Point, ...]
    stroke: str
    stroke_width: float
    opacity: float = 1.0


@dataclass(frozen=True)
class PolygonShape:
    """A filled and optionally stroked polygon."""

    element_id: str
    points: tuple[Point, ...]
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float = 0.0
    opacity: float = 1.0


Shape: TypeAlias = PathShape | EllipseShape | RoundedRectShape | LineShape | PolygonShape


@dataclass(frozen=True)
class EvdevCode:
    """Verified Linux input-event identity for one physical control."""

    name: str
    code: int


@dataclass(frozen=True)
class Slot:
    """One physical button and its marker/label geometry."""

    slot_id: str
    control: str
    physical_location: str
    evdev: EvdevCode
    marker: Point
    label_side: str


@dataclass(frozen=True)
class LightingZone:
    """A real, independently tintable lighting region."""

    zone_id: str
    shapes: tuple[Shape, ...]


@dataclass(frozen=True)
class DeviceSpec:
    """Complete data needed to emit one device's art and geometry."""

    device_id: str
    name: str
    kind: str
    canvas: tuple[int, int]
    physical_dimensions_mm: tuple[int, int, int]
    hidpp_model_ids: tuple[str, ...]
    usb_product_ids: tuple[str, ...]
    layers: tuple[Shape, ...]
    slots: tuple[Slot, ...]
    lighting_zones: tuple[LightingZone, ...]


def _p(x: float, y: float) -> Point:
    return Point(x, y)


BODY = PathShape(
    "device-body",
    _p(160, 18),
    (
        Cubic(_p(132, 17), _p(109, 29), _p(94, 55)),
        Cubic(_p(72, 91), _p(56, 153), _p(47, 224)),
        Cubic(_p(36, 300), _p(20, 365), _p(17, 426)),
        Cubic(_p(14, 478), _p(49, 516), _p(110, 538)),
        Cubic(_p(128, 544), _p(145, 546), _p(160, 546)),
        Cubic(_p(179, 546), _p(198, 541), _p(216, 534)),
        Cubic(_p(270, 513), _p(301, 478), _p(303, 433)),
        Cubic(_p(306, 367), _p(296, 300), _p(285, 228)),
        Cubic(_p(275, 155), _p(259, 92), _p(232, 54)),
        Cubic(_p(214, 28), _p(190, 17), _p(160, 18)),
    ),
    fill=DEEP_GRAPHITE,
    stroke=EDGE,
    stroke_width=3.0,
)

PALM_SHELL = PathShape(
    "palm-shell",
    _p(64, 223),
    (
        Cubic(_p(91, 215), _p(126, 212), _p(159, 214)),
        Cubic(_p(205, 210), _p(246, 218), _p(285, 238)),
        Cubic(_p(294, 302), _p(301, 370), _p(298, 427)),
        Cubic(_p(294, 470), _p(264, 504), _p(212, 526)),
        Cubic(_p(177, 540), _p(143, 541), _p(109, 530)),
        Cubic(_p(52, 511), _p(23, 477), _p(25, 429)),
        Cubic(_p(28, 365), _p(44, 291), _p(64, 223)),
    ),
    fill=GRAPHITE,
    stroke="#35506A",
    stroke_width=1.6,
)

LEFT_BUTTON = PathShape(
    "button-g1",
    _p(157, 26),
    (
        Cubic(_p(130, 25), _p(108, 36), _p(94, 61)),
        Cubic(_p(75, 94), _p(66, 143), _p(63, 195)),
        Cubic(_p(89, 204), _p(122, 207), _p(154, 202)),
        Cubic(_p(156, 143), _p(157, 84), _p(157, 26)),
    ),
    fill=RAISED_GRAPHITE,
    stroke="#4D6A82",
    stroke_width=1.8,
)

RIGHT_BUTTON = PathShape(
    "button-g2",
    _p(163, 26),
    (
        Cubic(_p(190, 24), _p(214, 35), _p(230, 61)),
        Cubic(_p(251, 96), _p(262, 144), _p(268, 198)),
        Cubic(_p(239, 206), _p(202, 208), _p(166, 202)),
        Cubic(_p(164, 142), _p(163, 84), _p(163, 26)),
    ),
    fill="#172B44",
    stroke="#4D6A82",
    stroke_width=1.8,
)

BASE_LAYERS: tuple[Shape, ...] = (
    BODY,
    PALM_SHELL,
    LEFT_BUTTON,
    RIGHT_BUTTON,
    LineShape("center-seam", (_p(160, 24), _p(160, 219)), "#6A879D", 1.6, 0.78),
    RoundedRectShape("wheel-well", (139, 62, 181, 177), 20, NIGHT, "#45657D", 1.8),
    RoundedRectShape("button-g3", (146, 70, 174, 168), 13, "#243A50", "#87A2B5", 1.4),
    LineShape("wheel-tread-1", (_p(150, 87), _p(170, 87)), "#7390A6", 2.0, 0.7),
    LineShape("wheel-tread-2", (_p(149, 104), _p(171, 104)), "#7390A6", 2.0, 0.7),
    LineShape("wheel-tread-3", (_p(149, 121), _p(171, 121)), "#7390A6", 2.0, 0.7),
    LineShape("wheel-tread-4", (_p(149, 138), _p(171, 138)), "#7390A6", 2.0, 0.7),
    LineShape("wheel-tread-5", (_p(150, 155), _p(170, 155)), "#7390A6", 2.0, 0.7),
    RoundedRectShape("button-g6", (142, 187, 178, 220), 12, "#203851", "#66849A", 1.5),
    LineShape("g6-index", (_p(151, 203), _p(169, 203)), CYAN, 1.5, 0.72),
    RoundedRectShape("button-g5", (28, 190, 82, 236), 14, "#1C3149", "#66849A", 1.5),
    RoundedRectShape("button-g4", (31, 252, 86, 302), 15, "#1A2E45", "#66849A", 1.5),
    LineShape("left-flank-seam", (_p(41, 317), _p(27, 404), _p(30, 450)), "#45657D", 1.4, 0.65),
    LineShape("right-technical-contour", (_p(278, 255), _p(290, 358), _p(286, 435)), "#45657D", 1.2, 0.58),
    LineShape("tail-axis", (_p(160, 243), _p(160, 498)), "#35506A", 1.0, 0.35),
)

LOGO_SHAPES: tuple[Shape, ...] = (
    LineShape("logo-bridge-left", (_p(160, 402), _p(144, 389)), CYAN, 4.0),
    LineShape("logo-bridge-right", (_p(160, 402), _p(177, 389)), CYAN, 4.0),
    LineShape("logo-bridge-top", (_p(160, 402), _p(160, 379)), CYAN, 4.0),
    LineShape("logo-bridge-tail", (_p(160, 402), _p(160, 427)), CYAN, 4.0),
    EllipseShape("logo-hub", (151, 393, 169, 411), CYAN),
    EllipseShape("logo-node-left", (138, 383, 150, 395), BLUE),
    EllipseShape("logo-node-right", (171, 383, 183, 395), CYAN),
    RoundedRectShape("logo-node-top", (155, 369, 165, 388), 5, BLUE),
    EllipseShape("logo-node-tail", (154, 421, 166, 433), CYAN),
)

DPI_SHAPES: tuple[Shape, ...] = (
    RoundedRectShape("dpi-track", (37, 326, 49, 401), 6, "#153C5C", "#456D85", 1.0),
    RoundedRectShape("dpi-lit-1", (40, 331, 46, 350), 3, BLUE),
    RoundedRectShape("dpi-lit-2", (40, 355, 46, 374), 3, CYAN),
    RoundedRectShape("dpi-lit-3", (40, 379, 46, 396), 3, CYAN, opacity=0.72),
)

G703 = DeviceSpec(
    device_id="g703_hero",
    name="Logitech G703 LIGHTSPEED HERO",
    kind="mouse",
    canvas=(320, 560),
    physical_dimensions_mm=(124, 68, 43),
    hidpp_model_ids=("4086",),
    usb_product_ids=("c090",),
    layers=BASE_LAYERS,
    slots=(
        Slot("g1", "left_click", "Left click", EvdevCode("BTN_LEFT", 272), _p(116, 112), "left"),
        Slot("g2", "right_click", "Right click", EvdevCode("BTN_RIGHT", 273), _p(205, 112), "right"),
        Slot("g3", "middle_click", "Wheel click", EvdevCode("BTN_MIDDLE", 274), _p(160, 130), "right"),
        Slot("g4", "back", "Rear side button", EvdevCode("BTN_SIDE", 275), _p(72, 286), "left"),
        Slot("g5", "forward", "Front side button", EvdevCode("BTN_EXTRA", 276), _p(67, 224), "left"),
        Slot("g6", "dpi_toggle", "DPI button", EvdevCode("BTN_TASK", 279), _p(160, 208), "right"),
    ),
    lighting_zones=(
        LightingZone("logo", LOGO_SHAPES),
        LightingZone("dpi_indicator", DPI_SHAPES),
    ),
)

DEVICE_SPECS = (G703,)


def _rgba(color: str, opacity: float = 1.0) -> tuple[int, int, int, int]:
    """Convert a CSS hex color and opacity to deterministic RGBA."""

    red, green, blue = ImageColor.getrgb(color)
    return red, green, blue, round(255 * opacity)


def _scale_point(point: Point, scale_x: float, scale_y: float) -> tuple[float, float]:
    return point.x * scale_x, point.y * scale_y


def _sample_path(path: PathShape, samples_per_segment: int = 24) -> list[Point]:
    """Sample a cubic path for Pillow without changing its canonical controls."""

    points = [path.start]
    current = path.start
    for segment in path.segments:
        for index in range(1, samples_per_segment + 1):
            t = index / samples_per_segment
            inverse = 1.0 - t
            points.append(
                Point(
                    inverse**3 * current.x
                    + 3 * inverse**2 * t * segment.control_1.x
                    + 3 * inverse * t**2 * segment.control_2.x
                    + t**3 * segment.end.x,
                    inverse**3 * current.y
                    + 3 * inverse**2 * t * segment.control_1.y
                    + 3 * inverse * t**2 * segment.control_2.y
                    + t**3 * segment.end.y,
                )
            )
        current = segment.end
    return points


def _draw_shape(
    draw: ImageDraw.ImageDraw,
    shape: Shape,
    scale_x: float,
    scale_y: float,
    mask: bool = False,
) -> None:
    """Draw one declarative shape on a supersampled Pillow canvas."""

    average_scale = (scale_x + scale_y) / 2.0
    fill_color = getattr(shape, "fill", None)
    stroke_color = getattr(shape, "stroke", None)
    stroke_width = getattr(shape, "stroke_width", 0.0)
    fill = (255, 255, 255, 255) if mask else (_rgba(fill_color, shape.opacity) if fill_color else None)
    stroke = (255, 255, 255, 255) if mask else (_rgba(stroke_color, shape.opacity) if stroke_color else None)
    width = max(1, round(stroke_width * average_scale))

    if isinstance(shape, PathShape):
        points = [_scale_point(point, scale_x, scale_y) for point in _sample_path(shape)]
        if fill is not None:
            draw.polygon(points, fill=fill)
        if stroke is not None:
            stroke_points = points + ([points[0]] if shape.closed else [])
            draw.line(stroke_points, fill=stroke, width=width, joint="curve")
        return

    if isinstance(shape, EllipseShape):
        left, top, right, bottom = shape.box
        draw.ellipse(
            (left * scale_x, top * scale_y, right * scale_x, bottom * scale_y),
            fill=fill,
            outline=stroke,
            width=width,
        )
        return

    if isinstance(shape, RoundedRectShape):
        left, top, right, bottom = shape.box
        draw.rounded_rectangle(
            (left * scale_x, top * scale_y, right * scale_x, bottom * scale_y),
            radius=shape.radius * average_scale,
            fill=fill,
            outline=stroke,
            width=width,
        )
        return

    if isinstance(shape, LineShape):
        points = [_scale_point(point, scale_x, scale_y) for point in shape.points]
        draw.line(points, fill=stroke, width=width, joint="curve")
        return

    if isinstance(shape, PolygonShape):
        points = [_scale_point(point, scale_x, scale_y) for point in shape.points]
        if fill is not None:
            draw.polygon(points, fill=fill)
        if stroke is not None:
            draw.line(points + [points[0]], fill=stroke, width=width, joint="curve")
        return

    raise TypeError(f"unsupported shape: {type(shape).__name__}")


def _render_raster(spec: DeviceSpec, height: int, zone: LightingZone | None = None) -> Image.Image:
    """Render a full device or one lighting-zone mask at *height*."""

    canvas_width, canvas_height = spec.canvas
    width = round(canvas_width / canvas_height * height)
    large_size = (width * SUPERSAMPLE, height * SUPERSAMPLE)
    image = Image.new("RGBA", large_size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    scale_x = large_size[0] / canvas_width
    scale_y = large_size[1] / canvas_height

    if zone is None:
        for shape in spec.layers:
            _draw_shape(draw, shape, scale_x, scale_y)
        for lighting_zone in spec.lighting_zones:
            for shape in lighting_zone.shapes:
                _draw_shape(draw, shape, scale_x, scale_y)
    else:
        for shape in zone.shapes:
            _draw_shape(draw, shape, scale_x, scale_y, mask=True)

    return image.resize((width, height), RESAMPLE)


def _number(value: float) -> str:
    """Format SVG numbers without locale or insignificant zeroes."""

    return f"{value:g}"


def _svg_paint(shape: Shape) -> str:
    attributes: list[str] = []
    fill = getattr(shape, "fill", None)
    stroke = getattr(shape, "stroke", None)
    stroke_width = getattr(shape, "stroke_width", 0.0)
    opacity = getattr(shape, "opacity", 1.0)
    attributes.append(f'fill="{fill if fill is not None else "none"}"')
    if stroke is not None:
        attributes.extend(
            (
                f'stroke="{stroke}"',
                f'stroke-width="{_number(stroke_width)}"',
                'stroke-linecap="round"',
                'stroke-linejoin="round"',
            )
        )
    if opacity != 1.0:
        attributes.append(f'opacity="{_number(opacity)}"')
    return " ".join(attributes)


def _shape_svg(shape: Shape, indent: str = "  ") -> str:
    """Serialize one shape from canonical data; this is not a renderer input."""

    paint = _svg_paint(shape)
    if isinstance(shape, PathShape):
        commands = [f"M {_number(shape.start.x)} {_number(shape.start.y)}"]
        commands.extend(
            "C "
            f"{_number(segment.control_1.x)} {_number(segment.control_1.y)} "
            f"{_number(segment.control_2.x)} {_number(segment.control_2.y)} "
            f"{_number(segment.end.x)} {_number(segment.end.y)}"
            for segment in shape.segments
        )
        if shape.closed:
            commands.append("Z")
        return f'{indent}<path id="{shape.element_id}" d="{" ".join(commands)}" {paint}/>'
    if isinstance(shape, EllipseShape):
        left, top, right, bottom = shape.box
        return (
            f'{indent}<ellipse id="{shape.element_id}" '
            f'cx="{_number((left + right) / 2)}" cy="{_number((top + bottom) / 2)}" '
            f'rx="{_number((right - left) / 2)}" ry="{_number((bottom - top) / 2)}" {paint}/>'
        )
    if isinstance(shape, RoundedRectShape):
        left, top, right, bottom = shape.box
        return (
            f'{indent}<rect id="{shape.element_id}" x="{_number(left)}" y="{_number(top)}" '
            f'width="{_number(right - left)}" height="{_number(bottom - top)}" '
            f'rx="{_number(shape.radius)}" {paint}/>'
        )
    if isinstance(shape, LineShape):
        points = " ".join(f"{_number(point.x)},{_number(point.y)}" for point in shape.points)
        return f'{indent}<polyline id="{shape.element_id}" points="{points}" {paint}/>'
    if isinstance(shape, PolygonShape):
        points = " ".join(f"{_number(point.x)},{_number(point.y)}" for point in shape.points)
        return f'{indent}<polygon id="{shape.element_id}" points="{points}" {paint}/>'
    raise TypeError(f"unsupported shape: {type(shape).__name__}")


def _device_svg(spec: DeviceSpec) -> str:
    width, height = spec.canvas
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">'
        ),
        f"  <title id=\"title\">{spec.name}</title>",
        "  <desc id=\"desc\">Original OpenHub top-down technical illustration.</desc>",
        '  <g id="device-art">',
    ]
    lines.extend(_shape_svg(shape, "    ") for shape in spec.layers)
    lines.append("  </g>")
    for zone in spec.lighting_zones:
        lines.append(f'  <g id="lighting-{zone.zone_id}" data-lighting-zone="{zone.zone_id}">')
        lines.extend(_shape_svg(shape, "    ") for shape in zone.shapes)
        lines.append("  </g>")
    lines.extend(("</svg>", ""))
    return "\n".join(lines)


def _geometry(spec: DeviceSpec) -> dict[str, object]:
    length, width, height = spec.physical_dimensions_mm
    return {
        "schema_version": 1,
        "device": {
            "id": spec.device_id,
            "name": spec.name,
            "kind": spec.kind,
            "canvas": {"width": spec.canvas[0], "height": spec.canvas[1]},
            "physical_dimensions_mm": {
                "length": length,
                "width": width,
                "height": height,
            },
            "identifiers": {
                "hidpp_model_ids": list(spec.hidpp_model_ids),
                "usb_product_ids": list(spec.usb_product_ids),
            },
        },
        "slots": [
            {
                "id": slot.slot_id,
                "control": slot.control,
                "physical_location": slot.physical_location,
                "evdev": {"name": slot.evdev.name, "code": slot.evdev.code},
                "marker": {"x": slot.marker.x, "y": slot.marker.y},
                "label_side": slot.label_side,
            }
            for slot in spec.slots
        ],
        "lighting_zones": [
            {
                "id": zone.zone_id,
                "svg_element": f"lighting-{zone.zone_id}",
                "masks": {
                    str(raster_height): f"lighting-{zone.zone_id}-{raster_height}.png"
                    for raster_height in RASTER_HEIGHTS
                },
            }
            for zone in spec.lighting_zones
        ],
    }


def _save_png(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=9)


def _output_paths(root: Path, spec: DeviceSpec) -> tuple[Path, ...]:
    directory = root / "design/devices" / spec.device_id
    paths = [directory / "device.svg", directory / "geometry.json"]
    for height in RASTER_HEIGHTS:
        paths.append(directory / f"device-{height}.png")
        paths.extend(
            directory / f"lighting-{zone.zone_id}-{height}.png"
            for zone in spec.lighting_zones
        )
    return tuple(sorted(paths))


def generate_device_assets(root: Path) -> list[Path]:
    """Generate every declared device below *root* and return sorted paths."""

    root = Path(root)
    written: list[Path] = []
    for spec in DEVICE_SPECS:
        output = root / "design/devices" / spec.device_id
        output.mkdir(parents=True, exist_ok=True)
        (output / "device.svg").write_text(_device_svg(spec), encoding="utf-8", newline="\n")
        (output / "geometry.json").write_text(
            json.dumps(_geometry(spec), indent=2, ensure_ascii=True) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        for height in RASTER_HEIGHTS:
            _save_png(_render_raster(spec, height), output / f"device-{height}.png")
            for zone in spec.lighting_zones:
                _save_png(
                    _render_raster(spec, height, zone),
                    output / f"lighting-{zone.zone_id}-{height}.png",
                )
        written.extend(_output_paths(root, spec))
    return sorted(written)


def validate_device_assets(root: Path) -> None:
    """Raise ``ValueError`` when generated device outputs violate the contract."""

    root = Path(root)
    for spec in DEVICE_SPECS:
        expected = _output_paths(root, spec)
        missing = [path for path in expected if not path.is_file()]
        if missing:
            raise ValueError(
                "missing generated device assets: "
                + ", ".join(path.relative_to(root).as_posix() for path in missing)
            )

        output = root / "design/devices" / spec.device_id
        geometry = json.loads((output / "geometry.json").read_text(encoding="utf-8"))
        if geometry.get("schema_version") != 1 or len(geometry.get("slots", [])) != len(spec.slots):
            raise ValueError(f"invalid geometry contract for {spec.device_id}")

        svg = ET.parse(output / "device.svg").getroot()
        if svg.attrib.get("viewBox") != f"0 0 {spec.canvas[0]} {spec.canvas[1]}":
            raise ValueError(f"invalid SVG canvas for {spec.device_id}")

        for height in RASTER_HEIGHTS:
            expected_size = (round(spec.canvas[0] / spec.canvas[1] * height), height)
            filenames = [f"device-{height}.png"] + [
                f"lighting-{zone.zone_id}-{height}.png" for zone in spec.lighting_zones
            ]
            for filename in filenames:
                with Image.open(output / filename) as image:
                    if image.format != "PNG" or image.mode != "RGBA" or image.size != expected_size:
                        raise ValueError(
                            f"invalid raster {filename}: format={image.format}, "
                            f"mode={image.mode}, size={image.size}"
                        )
                    if image.getchannel("A").getbbox() is None:
                        raise ValueError(f"empty raster: {filename}")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:12]


def main() -> None:
    root = Path(__file__).resolve().parents[2]
    written = generate_device_assets(root)
    validate_device_assets(root)
    print(f"Generated and validated {len(written)} OpenHub device assets:")
    for path in written:
        print(f"  {_sha256(path)}  {path.relative_to(root)}")


if __name__ == "__main__":
    main()

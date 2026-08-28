#!/usr/bin/env python3
"""Generate the original OpenHub "Circuito Orbital" brand asset set.

Pillow is the only non-standard-library dependency.  All raster derivatives are
resampled from 1024 px masters so the geometry has one canonical definition.
"""

from __future__ import annotations

import hashlib
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image, ImageChops, ImageColor, ImageDraw, ImageFilter, ImageFont


MASTER_SIZE = 1024
SMALL_ICON_SIZES = (16, 32, 48, 64, 128, 256, 512)
ICO_SIZES = (16, 32, 48, 64, 128, 256)
RESAMPLE = Image.Resampling.LANCZOS

NIGHT = "#08111F"
GRAPHITE = "#12233A"
BLUE = "#2563EB"
CYAN = "#22D3C5"
MINERAL = "#F2F7FA"
AMBER = "#F59E42"
CORAL = "#F05D5E"

GENERATED_PATHS = (
    "crates/openlogi-agent/assets/tray-icon-prism@2x.png",
    "crates/openlogi-agent/assets/tray-icon-white@2x.png",
    "crates/openlogi-agent/assets/tray-icon@2x.png",
    "crates/openlogi-desktop/icon/AppIcon.icns",
    "design/banner.png",
    "design/bg/openlogi-dmg-dark.svg",
    "design/bg/openlogi-dmg-light.svg",
    "design/icon/openlogi-128.png",
    "design/icon/openlogi-16.png",
    "design/icon/openlogi-256.png",
    "design/icon/openlogi-32.png",
    "design/icon/openlogi-48.png",
    "design/icon/openlogi-512.png",
    "design/icon/openlogi-64.png",
    "design/icon/openlogi-prism.ico",
    "design/icon/openlogi-prism.icon/Assets/OpenLogi-1.png",
    "design/icon/openlogi-prism.png",
    "design/icon/openlogi.ico",
    "design/icon/openlogi.icon/Assets/OpenLogi.png",
    "design/icon/openlogi.png",
)


def _rgba(color: str | tuple[int, ...]) -> tuple[int, int, int, int]:
    """Normalize a Pillow color value to RGBA."""

    if isinstance(color, str):
        return ImageColor.getrgb(color) + (255,)
    if len(color) == 3:
        return color + (255,)
    if len(color) == 4:
        return color
    raise ValueError("color must contain three or four channels")


def _vertical_gradient(
    size: tuple[int, int],
    top: str | tuple[int, ...],
    bottom: str | tuple[int, ...],
) -> Image.Image:
    """Return a deterministic vertical RGBA gradient."""

    ramp = Image.linear_gradient("L").resize(size, Image.Resampling.BILINEAR)
    return Image.composite(Image.new("RGBA", size, _rgba(bottom)), Image.new("RGBA", size, _rgba(top)), ramp)


def _scaled_box(box: tuple[int, int, int, int], scale: int) -> tuple[int, int, int, int]:
    return tuple(value * scale for value in box)  # type: ignore[return-value]


def _master_mark_mask(variant: str = "normal") -> Image.Image:
    """Render the canonical hub-and-six-nodes silhouette at 1024 px."""

    if variant not in {"normal", "prism"}:
        raise ValueError(f"unknown mark variant: {variant}")

    scale = 4
    canvas = Image.new("L", (MASTER_SIZE * scale, MASTER_SIZE * scale), 0)
    draw = ImageDraw.Draw(canvas)

    center = (520, 515)
    circular_nodes = (
        (348, 286, 92),  # primary left
        (684, 286, 92),  # primary right
        (252, 500, 66),  # side upper
        (270, 657, 62),  # side lower
        (530, 775, 61),  # auxiliary
    )
    wheel_center = (520, 202)

    def point(value: tuple[int, int]) -> tuple[int, int]:
        return value[0] * scale, value[1] * scale

    # Bridges are laid down before nodes so every branch reads as one silhouette.
    for target, width in (
        ((348, 286), 68),
        ((684, 286), 68),
        (wheel_center, 64),
        ((252, 500), 58),
        ((270, 657), 58),
        ((530, 775), 60),
    ):
        draw.line((point(center), point(target)), fill=255, width=width * scale)

    draw.ellipse(_scaled_box((372, 367, 668, 663), scale), fill=255)
    for x, y, radius in circular_nodes:
        draw.ellipse(
            _scaled_box((x - radius, y - radius, x + radius, y + radius), scale),
            fill=255,
        )
    draw.rounded_rectangle(_scaled_box((472, 112, 568, 292), scale), radius=48 * scale, fill=255)

    if variant == "prism":
        # A compact triangular aperture: circles soften its three vertices while
        # the central polygon keeps the opening broad enough to survive at 44 px.
        points = ((520, 440), (590, 562), (450, 562))
        scaled_points = tuple(point(value) for value in points)
        draw.polygon(scaled_points, fill=0)
        radius = 14 * scale
        for x, y in scaled_points:
            draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=0)

    return canvas.resize((MASTER_SIZE, MASTER_SIZE), RESAMPLE)


def render_mark(
    size: int,
    variant: str = "normal",
    color: tuple[int, int, int, int] = (0, 0, 0, 255),
) -> Image.Image:
    """Return a flat, monochrome RGBA mark on transparency."""

    if size <= 0:
        raise ValueError("size must be positive")
    mask = _master_mark_mask(variant)
    if size != MASTER_SIZE:
        mask = mask.resize((size, size), RESAMPLE)
    image = Image.new("RGBA", (size, size), _rgba(color))
    image.putalpha(mask)
    return image


def _plate_mask() -> Image.Image:
    scale = 4
    mask = Image.new("L", (MASTER_SIZE * scale, MASTER_SIZE * scale), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        _scaled_box((44, 36, 980, 972), scale),
        radius=226 * scale,
        fill=255,
    )
    return mask.resize((MASTER_SIZE, MASTER_SIZE), RESAMPLE)


def _faint_orbits(variant: str) -> Image.Image:
    overlay = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    accent = (34, 211, 197, 46) if variant == "normal" else (245, 158, 66, 52)
    secondary = (37, 99, 235, 30) if variant == "normal" else (240, 93, 94, 34)

    draw.arc((104, 92, 934, 916), 198, 352, fill=accent, width=3)
    draw.arc((146, 138, 888, 872), 20, 157, fill=secondary, width=2)
    draw.arc((236, 104, 810, 856), 214, 327, fill=accent, width=2)
    draw.line((106, 742, 268, 580), fill=secondary, width=2)
    draw.line((784, 150, 920, 286), fill=secondary, width=2)
    for x, y, radius in ((112, 742, 6), (920, 286, 5), (822, 846, 4), (190, 180, 4)):
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=accent)
    return overlay


def render_app_icon(variant: str = "normal") -> Image.Image:
    """Render a 1024 px OpenHub app icon without text."""

    if variant not in {"normal", "prism"}:
        raise ValueError(f"unknown app icon variant: {variant}")

    plate_mask = _plate_mask()
    result = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE), (0, 0, 0, 0))

    shadow_alpha = plate_mask.filter(ImageFilter.GaussianBlur(20)).point(
        tuple(value * 54 // 255 for value in range(256))
    )
    shadow = Image.new("RGBA", result.size, (0, 0, 0, 255))
    shadow.putalpha(shadow_alpha)
    result.alpha_composite(shadow, (0, 10))

    if variant == "normal":
        plate = _vertical_gradient(result.size, NIGHT, "#17304B")
        mark_colors = (BLUE, CYAN)
        glow_color = (34, 211, 197, 74)
    else:
        plate = _vertical_gradient(result.size, "#071923", "#17426A")
        mark_colors = (AMBER, CORAL)
        glow_color = (245, 158, 66, 78)
    plate.putalpha(plate_mask)
    result.alpha_composite(plate)

    orbit_layer = _faint_orbits(variant)
    orbit_layer.putalpha(ImageChops.multiply(orbit_layer.getchannel("A"), plate_mask))
    result.alpha_composite(orbit_layer)

    mark_mask = _master_mark_mask(variant)
    glow_alpha = mark_mask.filter(ImageFilter.GaussianBlur(30)).point(
        tuple(value * glow_color[3] // 255 for value in range(256))
    )
    glow = Image.new("RGBA", result.size, glow_color[:3] + (255,))
    glow.putalpha(glow_alpha)
    result.alpha_composite(glow)

    mark = _vertical_gradient(result.size, mark_colors[0], mark_colors[1])
    mark.putalpha(mark_mask)
    result.alpha_composite(mark)

    # One restrained edge catches light without turning the plate glossy.
    edge = Image.new("RGBA", result.size, (0, 0, 0, 0))
    ImageDraw.Draw(edge).arc((72, 62, 952, 946), 202, 318, fill=(242, 247, 250, 34), width=3)
    edge.putalpha(ImageChops.multiply(edge.getchannel("A"), plate_mask))
    result.alpha_composite(edge)
    return result


def _save_png(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", optimize=True, compress_level=9)


def _load_banner_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for name in ("DejaVuSans-Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default(size=size)


def _render_banner() -> Image.Image:
    size = (1280, 640)
    banner = _vertical_gradient(size, "#07111E", "#133451")

    technical = Image.new("RGBA", size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(technical)
    for inset, alpha in ((42, 24), (92, 18), (142, 12)):
        draw.arc((inset, -280 + inset, 1280 - inset, 910 - inset), 190, 350, fill=(34, 211, 197, alpha), width=2)
    draw.line((0, 512, 236, 512), fill=(37, 99, 235, 42), width=2)
    draw.line((1044, 128, 1280, 128), fill=(34, 211, 197, 42), width=2)
    for x, y in ((70, 94), (1190, 548), (1100, 128), (236, 512)):
        draw.ellipse((x - 5, y - 5, x + 5, y + 5), fill=(34, 211, 197, 50))
    banner.alpha_composite(technical)

    mark_mask = _master_mark_mask("normal").resize((520, 520), RESAMPLE)
    glow_alpha = mark_mask.filter(ImageFilter.GaussianBlur(22)).point(
        tuple(value * 74 // 255 for value in range(256))
    )
    glow = Image.new("RGBA", (520, 520), (34, 211, 197, 255))
    glow.putalpha(glow_alpha)
    banner.alpha_composite(glow, (34, 60))

    mark = _vertical_gradient((520, 520), BLUE, CYAN)
    mark.putalpha(mark_mask)
    banner.alpha_composite(mark, (34, 60))

    font = _load_banner_font(132)
    text = "OpenHub"
    text_draw = ImageDraw.Draw(banner)
    bounds = text_draw.textbbox((0, 0), text, font=font)
    text_height = bounds[3] - bounds[1]
    text_draw.text(
        (570, (640 - text_height) // 2 - bounds[1]),
        text,
        font=font,
        fill=_rgba(MINERAL),
    )
    return banner


def _dmg_svg(theme: str) -> str:
    if theme == "light":
        background, field, primary, secondary = "#F2F7FA", "#DCE8EF", BLUE, "#128C91"
    elif theme == "dark":
        background, field, primary, secondary = NIGHT, GRAPHITE, CYAN, BLUE
    else:
        raise ValueError(f"unknown DMG theme: {theme}")

    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="660" height="400" viewBox="0 0 660 400">
  <defs>
    <linearGradient id="field" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{background}"/>
      <stop offset="1" stop-color="{field}"/>
    </linearGradient>
    <radialGradient id="halo" cx="50%" cy="50%" r="50%">
      <stop offset="0" stop-color="{primary}" stop-opacity=".16"/>
      <stop offset="1" stop-color="{primary}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="660" height="400" fill="url(#field)"/>
  <ellipse cx="330" cy="204" rx="230" ry="172" fill="url(#halo)"/>
  <g fill="none" stroke="{primary}" stroke-linecap="round">
    <path d="M-28 310 C118 176 224 128 342 126 S562 196 688 60" opacity=".19" stroke-width="2"/>
    <path d="M-12 348 C152 214 250 184 356 182 S558 234 674 126" opacity=".11" stroke-width="1.5"/>
    <path d="M86 -30 C158 70 194 120 256 152" opacity=".16" stroke-width="2"/>
    <path d="M570 430 C518 344 486 302 426 270" opacity=".16" stroke-width="2"/>
  </g>
  <g fill="{secondary}" opacity=".22">
    <circle cx="28" cy="286" r="5"/><circle cx="116" cy="194" r="4"/>
    <circle cx="544" cy="164" r="4"/><circle cx="626" cy="84" r="5"/>
  </g>
  <g transform="translate(276 144) scale(.105)" fill="{primary}" opacity=".09">
    <path d="M520 515 L348 286 M520 515 L684 286 M520 515 L520 202 M520 515 L252 500 M520 515 L270 657 M520 515 L530 775" stroke="{primary}" stroke-width="72" stroke-linecap="round"/>
    <circle cx="520" cy="515" r="148"/><circle cx="348" cy="286" r="92"/><circle cx="684" cy="286" r="92"/>
    <rect x="472" y="112" width="96" height="180" rx="48"/><circle cx="252" cy="500" r="66"/>
    <circle cx="270" cy="657" r="62"/><circle cx="530" cy="775" r="61"/>
  </g>
  <rect x="18" y="18" width="624" height="364" rx="22" fill="none" stroke="{primary}" stroke-opacity=".10"/>
</svg>
'''


def generate_assets(root: Path) -> list[Path]:
    """Generate every brand asset below *root* and return its paths."""

    root = Path(root)
    normal = render_app_icon("normal")
    prism = render_app_icon("prism")

    _save_png(normal, root / "design/icon/openlogi.png")
    for size in SMALL_ICON_SIZES:
        _save_png(normal.resize((size, size), RESAMPLE), root / f"design/icon/openlogi-{size}.png")
    _save_png(prism, root / "design/icon/openlogi-prism.png")
    _save_png(normal, root / "design/icon/openlogi.icon/Assets/OpenLogi.png")
    _save_png(prism, root / "design/icon/openlogi-prism.icon/Assets/OpenLogi-1.png")

    normal_ico = root / "design/icon/openlogi.ico"
    prism_ico = root / "design/icon/openlogi-prism.ico"
    normal.save(normal_ico, format="ICO", sizes=tuple((size, size) for size in ICO_SIZES))
    prism.save(prism_ico, format="ICO", sizes=tuple((size, size) for size in ICO_SIZES))

    icns_path = root / "crates/openlogi-desktop/icon/AppIcon.icns"
    icns_path.parent.mkdir(parents=True, exist_ok=True)
    normal.save(icns_path, format="ICNS")

    _save_png(render_mark(44, "normal", (0, 0, 0, 255)), root / "crates/openlogi-agent/assets/tray-icon@2x.png")
    _save_png(render_mark(44, "normal", (255, 255, 255, 255)), root / "crates/openlogi-agent/assets/tray-icon-white@2x.png")
    _save_png(render_mark(44, "prism", (0, 0, 0, 255)), root / "crates/openlogi-agent/assets/tray-icon-prism@2x.png")

    _save_png(_render_banner(), root / "design/banner.png")
    for theme in ("light", "dark"):
        svg_path = root / f"design/bg/openlogi-dmg-{theme}.svg"
        svg_path.parent.mkdir(parents=True, exist_ok=True)
        svg_path.write_text(_dmg_svg(theme), encoding="utf-8", newline="\n")

    return [root / relative for relative in GENERATED_PATHS]


def validate_assets(root: Path) -> None:
    """Raise ``ValueError`` when any generated asset violates its contract."""

    root = Path(root)
    missing = [relative for relative in GENERATED_PATHS if not (root / relative).is_file()]
    if missing:
        raise ValueError(f"missing generated assets: {', '.join(missing)}")

    expected_png_sizes = {
        "design/icon/openlogi.png": (1024, 1024),
        "design/icon/openlogi-prism.png": (1024, 1024),
        "design/icon/openlogi.icon/Assets/OpenLogi.png": (1024, 1024),
        "design/icon/openlogi-prism.icon/Assets/OpenLogi-1.png": (1024, 1024),
        "design/banner.png": (1280, 640),
        "crates/openlogi-agent/assets/tray-icon@2x.png": (44, 44),
        "crates/openlogi-agent/assets/tray-icon-white@2x.png": (44, 44),
        "crates/openlogi-agent/assets/tray-icon-prism@2x.png": (44, 44),
    }
    expected_png_sizes.update(
        {f"design/icon/openlogi-{size}.png": (size, size) for size in SMALL_ICON_SIZES}
    )
    for relative, expected_size in expected_png_sizes.items():
        with Image.open(root / relative) as image:
            if image.format != "PNG" or image.size != expected_size or image.mode != "RGBA":
                raise ValueError(
                    f"invalid PNG {relative}: format={image.format}, size={image.size}, mode={image.mode}"
                )

    expected_tray_colors = {
        "crates/openlogi-agent/assets/tray-icon@2x.png": (0, 0, 0),
        "crates/openlogi-agent/assets/tray-icon-white@2x.png": (255, 255, 255),
        "crates/openlogi-agent/assets/tray-icon-prism@2x.png": (0, 0, 0),
    }
    for relative, expected_color in expected_tray_colors.items():
        with Image.open(root / relative).convert("RGBA") as image:
            pixels = list(image.get_flattened_data())
            colors = {pixel[:3] for pixel in pixels if pixel[3]}
            alphas = {pixel[3] for pixel in pixels}
            if colors != {expected_color} or 0 not in alphas or max(alphas) != 255:
                raise ValueError(f"tray icon is not flat monochrome on transparency: {relative}")

    expected_ico_sizes = {(size, size) for size in ICO_SIZES}
    for relative in ("design/icon/openlogi.ico", "design/icon/openlogi-prism.ico"):
        with Image.open(root / relative) as image:
            actual_sizes = set(image.ico.sizes())
            if image.format != "ICO" or actual_sizes != expected_ico_sizes:
                raise ValueError(f"invalid ICO {relative}: sizes={sorted(actual_sizes)}")

    with Image.open(root / "crates/openlogi-desktop/icon/AppIcon.icns") as image:
        if image.format != "ICNS" or image.size != (1024, 1024):
            raise ValueError(f"invalid ICNS: format={image.format}, size={image.size}")

    for theme in ("light", "dark"):
        relative = f"design/bg/openlogi-dmg-{theme}.svg"
        svg_root = ET.parse(root / relative).getroot()
        if (
            svg_root.attrib.get("width") != "660"
            or svg_root.attrib.get("height") != "400"
            or svg_root.attrib.get("viewBox") != "0 0 660 400"
        ):
            raise ValueError(f"invalid SVG canvas: {relative}")

    tiny = [relative for relative in GENERATED_PATHS if (root / relative).stat().st_size < 200]
    if tiny:
        raise ValueError(f"unexpectedly small generated assets: {', '.join(tiny)}")


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:12]


def main() -> None:
    root = Path(__file__).resolve().parents[2]
    written = generate_assets(root)
    validate_assets(root)
    print(f"Generated and validated {len(written)} OpenHub assets:")
    for path in written:
        print(f"  {_sha256(path)}  {path.relative_to(root)}")


if __name__ == "__main__":
    main()

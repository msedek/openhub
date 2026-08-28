"""Behavior tests for the OpenHub device asset generator."""

from __future__ import annotations

import hashlib
import importlib
import json
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image, ImageChops


EXPECTED_OUTPUTS = (
    "design/devices/g703_hero/device-120.png",
    "design/devices/g703_hero/device-320.png",
    "design/devices/g703_hero/device.svg",
    "design/devices/g703_hero/geometry.json",
    "design/devices/g703_hero/lighting-dpi_indicator-120.png",
    "design/devices/g703_hero/lighting-dpi_indicator-320.png",
    "design/devices/g703_hero/lighting-logo-120.png",
    "design/devices/g703_hero/lighting-logo-320.png",
)

EXPECTED_SLOTS = (
    ("g1", "left_click", "BTN_LEFT", 272, (116, 112), "left"),
    ("g2", "right_click", "BTN_RIGHT", 273, (205, 112), "right"),
    ("g3", "middle_click", "BTN_MIDDLE", 274, (160, 130), "right"),
    ("g4", "back", "BTN_SIDE", 275, (72, 286), "left"),
    ("g5", "forward", "BTN_EXTRA", 276, (67, 224), "left"),
    ("g6", "dpi_toggle", "BTN_TASK", 279, (160, 208), "right"),
)

EXPECTED_RASTERS = {
    "device-120.png": (69, 120),
    "device-320.png": (183, 320),
    "lighting-dpi_indicator-120.png": (69, 120),
    "lighting-dpi_indicator-320.png": (183, 320),
    "lighting-logo-120.png": (69, 120),
    "lighting-logo-320.png": (183, 320),
}


def load_generator():
    """Import production code while making a missing module an assertion failure."""

    try:
        return importlib.import_module("tools.brand.generate_device_assets")
    except ModuleNotFoundError as error:
        raise AssertionError("device asset generator module is missing") from error


def sha256(path: Path) -> str:
    """Return the full digest for a generated file."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


class GenerateDeviceAssetsTests(unittest.TestCase):
    """Exercise the real generated artifact contract in temporary roots."""

    def test_generate_device_assets_writes_complete_contract(self) -> None:
        generator = load_generator()

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            written = generator.generate_device_assets(root)
            actual = tuple(sorted(path.relative_to(root).as_posix() for path in written))

            self.assertEqual(EXPECTED_OUTPUTS, actual)
            self.assertTrue(all(path.is_file() for path in written))

    def test_geometry_carries_verified_slots_and_named_lighting_zones(self) -> None:
        generator = load_generator()

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generator.generate_device_assets(root)
            geometry = json.loads(
                (root / "design/devices/g703_hero/geometry.json").read_text(
                    encoding="utf-8"
                )
            )

            device = geometry["device"]
            self.assertEqual(1, geometry["schema_version"])
            self.assertEqual("g703_hero", device["id"])
            self.assertEqual({"width": 320, "height": 560}, device["canvas"])
            self.assertEqual(
                {"length": 124, "width": 68, "height": 43},
                device["physical_dimensions_mm"],
            )
            self.assertEqual(["4086"], device["identifiers"]["hidpp_model_ids"])
            self.assertEqual(["c090"], device["identifiers"]["usb_product_ids"])

            actual_slots = tuple(
                (
                    slot["id"],
                    slot["control"],
                    slot["evdev"]["name"],
                    slot["evdev"]["code"],
                    (slot["marker"]["x"], slot["marker"]["y"]),
                    slot["label_side"],
                )
                for slot in geometry["slots"]
            )
            self.assertEqual(EXPECTED_SLOTS, actual_slots)
            self.assertEqual(
                ("logo", "dpi_indicator"),
                tuple(zone["id"] for zone in geometry["lighting_zones"]),
            )

    def test_svg_and_png_outputs_share_the_declared_canvas(self) -> None:
        generator = load_generator()

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generator.generate_device_assets(root)
            output = root / "design/devices/g703_hero"

            svg = ET.parse(output / "device.svg").getroot()
            self.assertEqual("320", svg.attrib["width"])
            self.assertEqual("560", svg.attrib["height"])
            self.assertEqual("0 0 320 560", svg.attrib["viewBox"])
            element_ids = {
                element.attrib["id"]
                for element in svg.iter()
                if "id" in element.attrib
            }
            self.assertIn("device-body", element_ids)
            self.assertIn("lighting-logo", element_ids)
            self.assertIn("lighting-dpi_indicator", element_ids)

            for filename, expected_size in EXPECTED_RASTERS.items():
                with Image.open(output / filename) as image:
                    self.assertEqual("PNG", image.format)
                    self.assertEqual("RGBA", image.mode)
                    self.assertEqual(expected_size, image.size)
                    alpha = image.getchannel("A")
                    self.assertIsNotNone(alpha.getbbox(), filename)

    def test_lighting_masks_are_separate_and_contained_by_the_device(self) -> None:
        generator = load_generator()

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generator.generate_device_assets(root)
            output = root / "design/devices/g703_hero"

            with (
                Image.open(output / "device-320.png").convert("RGBA") as device,
                Image.open(output / "lighting-logo-320.png").convert("RGBA") as logo,
                Image.open(output / "lighting-dpi_indicator-320.png").convert("RGBA") as dpi,
            ):
                device_alpha = device.getchannel("A")
                logo_alpha = logo.getchannel("A")
                dpi_alpha = dpi.getchannel("A")
                binary = lambda alpha: alpha.point(lambda value: 255 if value else 0)
                self.assertIsNone(
                    ImageChops.subtract(binary(logo_alpha), binary(device_alpha)).getbbox()
                )
                self.assertIsNone(
                    ImageChops.subtract(binary(dpi_alpha), binary(device_alpha)).getbbox()
                )
                self.assertIsNone(ImageChops.multiply(logo_alpha, dpi_alpha).getbbox())
                self.assertNotEqual(logo_alpha.getbbox(), dpi_alpha.getbbox())

    def test_generation_is_byte_for_byte_deterministic(self) -> None:
        generator = load_generator()

        with (
            tempfile.TemporaryDirectory() as first_directory,
            tempfile.TemporaryDirectory() as second_directory,
        ):
            first_root = Path(first_directory)
            second_root = Path(second_directory)
            first = generator.generate_device_assets(first_root)
            second = generator.generate_device_assets(second_root)

            first_digests = {
                path.relative_to(first_root).as_posix(): sha256(path) for path in first
            }
            second_digests = {
                path.relative_to(second_root).as_posix(): sha256(path) for path in second
            }
            self.assertEqual(first_digests, second_digests)


if __name__ == "__main__":
    unittest.main()

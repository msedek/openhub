"""Behavior tests for the OpenHub brand asset generator."""

from __future__ import annotations

import importlib
import tempfile
import unittest
from pathlib import Path


EXPECTED_OUTPUTS = (
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


def load_generator():
    """Import the production module while keeping the missing-module failure clear."""

    try:
        return importlib.import_module("tools.brand.generate_assets")
    except ModuleNotFoundError as error:
        raise AssertionError("brand asset generator module is missing") from error


class GenerateAssetsTests(unittest.TestCase):
    """Exercise the real filesystem contract in an isolated destination root."""

    def test_generate_assets_writes_the_complete_output_contract(self) -> None:
        generator = load_generator()

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            written = generator.generate_assets(root)
            relative = tuple(sorted(path.relative_to(root).as_posix() for path in written))

            self.assertEqual(EXPECTED_OUTPUTS, relative)
            self.assertTrue(all(path.is_file() for path in written))


if __name__ == "__main__":
    unittest.main()

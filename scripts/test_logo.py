#!/usr/bin/env python3
"""Check exported artwork, not merely the generator's implementation."""
import hashlib
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

GENERATOR = Path(__file__).with_name("logo.py")
NS = {"s": "http://www.w3.org/2000/svg"}


class LogoExports(unittest.TestCase):
    def run_generator(self, output, *args):
        return subprocess.run([sys.executable, str(GENERATOR), "--output-dir", str(output), *args],
                              capture_output=True, text=True)

    def test_colors_alpha_turnstile_and_raster_sizes(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            args = ("--mark-color", "#123456", "--shadow-color", "#654321", "--shadow-opacity", ".4", "--sizes", "32", "96")
            result = self.run_generator(output, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            svg = ET.parse(output / "logo.svg").getroot()
            mark = svg.find("s:g[@data-layer='mark']", NS)
            assert mark is not None
            self.assertEqual(mark.attrib["fill"], "#123456")
            rects = mark.findall("s:rect", NS)
            # A left vertical spine and a central rightward arm, not an upright T.
            self.assertEqual(len({r.attrib["x"] for r in rects}), 1)
            arm = max(rects, key=lambda r: float(r.attrib["width"]))
            self.assertGreater(float(arm.attrib["y"]), min(float(r.attrib["y"]) for r in rects))
            self.assertLess(float(arm.attrib["y"]), max(float(r.attrib["y"]) for r in rects))
            size = float(svg.attrib["viewBox"].split()[2])
            x0 = min(float(r.attrib["x"]) for r in rects)
            x1 = max(float(r.attrib["x"]) + float(r.attrib["width"]) for r in rects)
            y0 = min(float(r.attrib["y"]) for r in rects)
            y1 = max(float(r.attrib["y"]) + float(r.attrib["height"]) for r in rects)
            self.assertAlmostEqual((x0 + x1) / 2, size / 2, places=1)
            self.assertAlmostEqual((y0 + y1) / 2, size / 2, places=1)
            for shadow in svg.findall("s:path[@data-layer='shadow']", NS):
                self.assertEqual(shadow.attrib["fill"], "#654321")
                self.assertGreater(float(shadow.attrib["fill-opacity"]), 0)
                self.assertLessEqual(float(shadow.attrib["fill-opacity"]), .4)
            self.assertNotIn("prefers-color-scheme", (output / "logo.svg").read_text())
            for size in (32, 96):
                png = (output / f"logo-{size}.png").read_bytes()
                self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(struct.unpack(">II", png[16:24]), (size, size))
                self.assertEqual(png[25], 6, "PNG must retain RGBA transparency")
            before = hashlib.sha256((output / "logo.svg").read_bytes()).digest()
            self.assertEqual(self.run_generator(output, *args).returncode, 0)
            self.assertEqual(before, hashlib.sha256((output / "logo.svg").read_bytes()).digest())

    def test_png_only_and_invalid_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = self.run_generator(output, "--variant", "logo", "--format", "png", "--sizes", "32")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(sorted(p.name for p in output.iterdir()), ["logo-32.png"])
            for args in [("--shadow-opacity", "nan"), ("--shadow-opacity", "1.1"),
                         ("--mark-color", "<script>"), ("--sizes", "0")]:
                self.assertNotEqual(self.run_generator(output, *args).returncode, 0)

    def test_svg_titles_are_escaped(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = self.run_generator(output, "--format", "svg", "--title", '<component & "one">')
            self.assertEqual(result.returncode, 0, result.stderr)
            root = ET.parse(output / "logo.svg").getroot()
            self.assertEqual(root.attrib["aria-label"], '<component & "one">')
            title = root.find("s:title", NS)
            assert title is not None
            self.assertEqual(title.text, '<component & "one">')

    def test_base_theme_and_less_shadow_mark(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            result = self.run_generator(output, "--format", "svg")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("prefers-color-scheme:dark", (output / "logo.svg").read_text())
            logo = ET.parse(output / "logo.svg").getroot()
            mark = ET.parse(output / "mark.svg").getroot()
            self.assertEqual(len(logo.findall("s:path[@data-layer='shadow']", NS)),3)
            self.assertEqual(len(mark.findall("s:path[@data-layer='shadow']", NS)),1)

    def test_component_generation_excludes_linen_and_infra(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for component in ("ledger", "liaison", "lode", "lun", "web-data", "secrets", "linen", "infra", "typednotes-infra"):
                (root / component).mkdir()
                (root / component / "logo.svg").write_text("original")
            result = self.run_generator(root / "output", "--components-root", str(root), "--format", "svg")
            self.assertEqual(result.returncode, 0, result.stderr)
            for component in ("linen", "infra", "typednotes-infra"):
                self.assertEqual((root / component / "logo.svg").read_text(), "original")
                self.assertNotEqual(self.run_generator(root / "output", "--components-root", str(root), "--components", component, "--format", "svg").returncode, 0)
            self.assertIn('data-layer="mark"', (root / "ledger" / "logo.svg").read_text())
            for component, foreground, shadow in [
                ("ledger", "#1f5c3f", "#b8892b"), ("liaison", "#0e6b6f", "#e0763a"),
                ("lode", "#3730a3", "#c2417a"), ("lun", "#3b2f7a", "#e0a526"),
            ]:
                svg = ET.parse(root / component / "logo.svg").getroot()
                mark = svg.find("s:g[@data-layer='mark']", NS)
                assert mark is not None
                self.assertEqual(mark.attrib["fill"], foreground)
                bands = svg.findall("s:path[@data-layer='shadow']", NS)
                self.assertTrue(all(band.attrib["fill"] == shadow for band in bands))
                self.assertEqual(max(float(band.attrib["fill-opacity"]) for band in bands), 1.0)
                self.assertNotIn("prefers-color-scheme", (root / component / "logo.svg").read_text())


if __name__ == "__main__":
    unittest.main()

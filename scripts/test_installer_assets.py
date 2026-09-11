"""The 125% MSIX images use Microsoft's rounded-up pixel dimensions."""

import importlib
import unittest

assets = importlib.import_module("gen-installer-assets")


class InstallerAssetTests(unittest.TestCase):
    def test_msix_125_percent_dimensions(self):
        for name, size in (
            ("Square150x150Logo.scale-125.png", 188),
            ("StoreLogo.scale-125.png", 63),
        ):
            with self.subTest(asset=name):
                self.assertEqual(assets.MSIX_ASSETS[name], size)
                self.assertIsNone(
                    assets.check_png(assets.MSIX_DIR / name, (size, size), color=6)
                )


if __name__ == "__main__":
    unittest.main()

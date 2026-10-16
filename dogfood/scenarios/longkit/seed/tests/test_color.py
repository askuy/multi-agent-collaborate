import unittest

from kit._stub import is_stub
from kit.color import hex_to_rgb, rgb_to_hex


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (hex_to_rgb, rgb_to_hex,)):
            self.skipTest("not implemented yet")

    def test_color(self):
        self.assertEqual(hex_to_rgb("#ff8800"), (255, 136, 0))
        self.assertEqual(hex_to_rgb("#F80"), (255, 136, 0))
        for bad in ("ff8800", "#ggg", "#12345"):
            with self.assertRaises(ValueError):
                hex_to_rgb(bad)
        self.assertEqual(rgb_to_hex(255, 136, 0), "#ff8800")
        with self.assertRaises(ValueError):
            rgb_to_hex(256, 0, 0)

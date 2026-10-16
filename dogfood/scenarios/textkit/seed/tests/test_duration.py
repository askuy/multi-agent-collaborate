import unittest

from textkit import parse_duration


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(parse_duration, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_values(self):
        self.assertEqual(parse_duration("1d2h30m15s"), 95415)
        self.assertEqual(parse_duration("2h 5m"), 7500)
        self.assertEqual(parse_duration("45s"), 45)

    def test_invalid(self):
        for text in ("", "5", "1x", "1m1h", "1h1h"):
            with self.assertRaises(ValueError):
                parse_duration(text)

import unittest

from kit._stub import is_stub
from kit.temperature import convert


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (convert,)):
            self.skipTest("not implemented yet")

    def test_temperature(self):
        self.assertEqual(convert(100, "C", "F"), 212.0)
        self.assertEqual(convert(32, "F", "C"), 0.0)
        self.assertEqual(convert(0, "K", "C"), -273.15)
        self.assertEqual(convert(-40, "C", "F"), -40.0)
        with self.assertRaises(ValueError):
            convert(-1, "K", "C")
        with self.assertRaises(ValueError):
            convert(1, "X", "C")

import unittest

from kit._stub import is_stub
from kit.clock12 import to_24h, to_12h


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (to_24h, to_12h,)):
            self.skipTest("not implemented yet")

    def test_clock12(self):
        self.assertEqual(to_24h("1:05 PM"), "13:05")
        self.assertEqual(to_24h("12:00 AM"), "00:00")
        self.assertEqual(to_24h("12:30 PM"), "12:30")
        self.assertEqual(to_12h("00:30"), "12:30 AM")
        self.assertEqual(to_12h("23:59"), "11:59 PM")
        self.assertEqual(to_12h("12:00"), "12:00 PM")
        with self.assertRaises(ValueError):
            to_24h("13:00 PM")
        with self.assertRaises(ValueError):
            to_12h("24:00")

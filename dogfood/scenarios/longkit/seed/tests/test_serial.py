import unittest

from kit._stub import is_stub
from kit.serial import next_id


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (next_id,)):
            self.skipTest("not implemented yet")

    def test_serial(self):
        self.assertEqual(next_id("INV-0099"), "INV-0100")
        self.assertEqual(next_id("INV-9999"), "INV-10000")
        self.assertEqual(next_id("A7"), "A8")
        self.assertEqual(next_id("X-009"), "X-010")
        with self.assertRaises(ValueError):
            next_id("INV-")

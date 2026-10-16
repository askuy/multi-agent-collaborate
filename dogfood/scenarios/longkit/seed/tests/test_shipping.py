import unittest

from kit._stub import is_stub
from kit.shipping import cost


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (cost,)):
            self.skipTest("not implemented yet")

    def test_shipping(self):
        self.assertEqual(cost(1, "domestic"), 5.0)
        self.assertEqual(cost(1.2, "domestic"), 6.5)
        self.assertEqual(cost(3, "eu"), 14.0)
        self.assertEqual(cost(0.5, "world"), 15.0)
        with self.assertRaises(ValueError):
            cost(31, "eu")
        with self.assertRaises(ValueError):
            cost(1, "mars")

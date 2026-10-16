import unittest

from kit._stub import is_stub
from kit.coupons import apply


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (apply,)):
            self.skipTest("not implemented yet")

    def test_coupons(self):
        self.assertEqual(apply(10000, ["10%"]), 9000)
        self.assertEqual(apply(10000, ["10%", "20%"]), 8000)
        self.assertEqual(apply(10000, ["20%", "500", "300"]), 7200)
        self.assertEqual(apply(1000, ["2000"]), 0)
        self.assertEqual(apply(999, ["50%"]), 499)
        with self.assertRaises(ValueError):
            apply(100, ["abc"])

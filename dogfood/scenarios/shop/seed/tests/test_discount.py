import inspect
import unittest

from shop import discount
from shop._stub import is_stub
from shop.discount import apply_discount


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (apply_discount,)):
            self.skipTest("not implemented yet (or a function it uses is not)")

    def test_uses_shop_money(self):
        self.assertRegex(inspect.getsource(discount), r"\bmoney\b")

    def test_apply_discount(self):
        self.assertEqual(apply_discount("10.00", 15), "8.50")
        self.assertEqual(apply_discount("0.99", 50), "0.49")
        self.assertEqual(apply_discount("5.00", 0), "5.00")
        self.assertEqual(apply_discount("5.00", 100), "0.00")

import inspect
import unittest

from shop import tax
from shop._stub import is_stub
from shop.tax import add_tax


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (add_tax,)):
            self.skipTest("not implemented yet (or a function it uses is not)")

    def test_uses_shop_money(self):
        self.assertRegex(inspect.getsource(tax), r"\bmoney\b")

    def test_add_tax(self):
        self.assertEqual(add_tax("10.00", 8), "10.80")
        self.assertEqual(add_tax("0.10", 5), "0.11")
        self.assertEqual(add_tax("19.99", 7), "21.39")
        self.assertEqual(add_tax("3.00", 0), "3.00")

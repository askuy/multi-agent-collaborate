import inspect
import unittest

from shop import receipt
from shop._stub import is_stub
from shop.receipt import render
from shop.tax import add_tax


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (render, add_tax,)):
            self.skipTest("not implemented yet (or a function it uses is not)")

    def test_uses_shop_money(self):
        self.assertRegex(inspect.getsource(receipt), r"\bmoney\b")

    def test_render(self):
        self.assertEqual(
            render([("apple", "1.20"), ("pear", "0.80")], 8),
            "apple 1.20\npear 0.80\nsubtotal 2.00\ntotal 2.16\n",
        )
        self.assertEqual(render([], 8), "subtotal 0.00\ntotal 0.00\n")

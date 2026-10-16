import inspect
import unittest

from shop import split
from shop._stub import is_stub
from shop.split import split_bill


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (split_bill,)):
            self.skipTest("not implemented yet (or a function it uses is not)")

    def test_uses_shop_money(self):
        self.assertRegex(inspect.getsource(split), r"\bmoney\b")

    def test_split_bill(self):
        self.assertEqual(split_bill("10.00", 3), ["3.34", "3.33", "3.33"])
        self.assertEqual(split_bill("0.05", 2), ["0.03", "0.02"])
        self.assertEqual(split_bill("9.00", 1), ["9.00"])

import unittest

from kit._stub import is_stub
from kit.rpn import evaluate


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (evaluate,)):
            self.skipTest("not implemented yet")

    def test_rpn(self):
        self.assertEqual(evaluate("3 4 + 2 *"), 14)
        self.assertEqual(evaluate("7 -2 /"), -3)
        self.assertEqual(evaluate("5"), 5)
        for bad in ("1 +", "1 2", "1 x +", ""):
            with self.assertRaises(ValueError):
                evaluate(bad)
        with self.assertRaises(ZeroDivisionError):
            evaluate("1 0 /")

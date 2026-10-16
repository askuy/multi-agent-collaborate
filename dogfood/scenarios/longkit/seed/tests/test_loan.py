import unittest

from kit._stub import is_stub
from kit.loan import payment


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (payment,)):
            self.skipTest("not implemented yet")

    def test_loan(self):
        self.assertEqual(payment(1000, 0, 4), 250.0)
        self.assertEqual(payment(200000, 6, 360), 1199.10)
        self.assertEqual(payment(10000, 12, 12), 888.49)
        with self.assertRaises(ValueError):
            payment(1000, 5, 0)

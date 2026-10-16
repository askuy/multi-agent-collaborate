import unittest

from kit._stub import is_stub
from kit.bizdays import add_business_days


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (add_business_days,)):
            self.skipTest("not implemented yet")

    def test_bizdays(self):
        self.assertEqual(add_business_days("2024-01-05", 1), "2024-01-08")
        self.assertEqual(add_business_days("2024-01-08", -1), "2024-01-05")
        self.assertEqual(add_business_days("2024-01-01", 10), "2024-01-15")
        self.assertEqual(add_business_days("2024-01-06", 1), "2024-01-08")
        self.assertEqual(add_business_days("2024-01-03", 0), "2024-01-03")
        self.assertEqual(add_business_days("2024-02-28", 2), "2024-03-01")

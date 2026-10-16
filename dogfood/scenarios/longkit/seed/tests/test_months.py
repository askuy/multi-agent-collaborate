import unittest

from kit._stub import is_stub
from kit.months import is_leap, days_in_month


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (is_leap, days_in_month,)):
            self.skipTest("not implemented yet")

    def test_months(self):
        self.assertTrue(is_leap(2000))
        self.assertFalse(is_leap(1900))
        self.assertTrue(is_leap(2024))
        self.assertFalse(is_leap(2023))
        self.assertEqual(days_in_month(2024, 2), 29)
        self.assertEqual(days_in_month(2023, 2), 28)
        self.assertEqual(days_in_month(2023, 4), 30)
        with self.assertRaises(ValueError):
            days_in_month(2023, 13)

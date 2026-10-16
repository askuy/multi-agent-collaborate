import unittest

from shop import money


class Test(unittest.TestCase):
    def setUp(self):
        if money.API_VERSION < 2:
            self.skipTest("shop.money is still on version 1")

    def test_to_cents(self):
        self.assertEqual(money.to_cents("12.34"), 1234)
        self.assertEqual(money.to_cents("0.05"), 5)
        self.assertEqual(money.to_cents("7"), 700)
        self.assertEqual(money.to_cents("-1.50"), -150)

    def test_format_cents(self):
        self.assertEqual(money.format_cents(1234), "12.34")
        self.assertEqual(money.format_cents(5), "0.05")
        self.assertEqual(money.format_cents(-150), "-1.50")
        with self.assertRaises(TypeError):
            money.format_cents(1.5)

    def test_version_1_names_are_gone(self):
        self.assertFalse(hasattr(money, "parse_amount"))
        self.assertFalse(hasattr(money, "format_amount"))

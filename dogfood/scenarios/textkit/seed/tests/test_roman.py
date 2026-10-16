import unittest

from textkit import roman


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(roman, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_values(self):
        for n, s in [(1, "I"), (4, "IV"), (9, "IX"), (40, "XL"), (1994, "MCMXCIV"), (3999, "MMMCMXCIX")]:
            self.assertEqual(roman(n), s)

    def test_range(self):
        for n in (0, -1, 4000):
            with self.assertRaises(ValueError):
                roman(n)

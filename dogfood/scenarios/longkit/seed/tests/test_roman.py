import unittest

from kit._stub import is_stub
from kit.roman import to_roman, from_roman


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (to_roman, from_roman,)):
            self.skipTest("not implemented yet")

    def test_roman(self):
        self.assertEqual(to_roman(1994), "MCMXCIV")
        self.assertEqual(to_roman(3999), "MMMCMXCIX")
        self.assertEqual(from_roman("MCMXCIV"), 1994)
        self.assertEqual(from_roman("IV"), 4)
        for bad in ("IIII", "", "IC", "ABC"):
            with self.assertRaises(ValueError):
                from_roman(bad)
        for bad in (0, 4000):
            with self.assertRaises(ValueError):
                to_roman(bad)

import unittest

from kit._stub import is_stub
from kit.series import largest_product


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (largest_product,)):
            self.skipTest("not implemented yet")

    def test_series(self):
        self.assertEqual(largest_product("63915", 3), 162)
        self.assertEqual(largest_product("1027839564", 3), 270)
        self.assertEqual(largest_product("", 0), 1)
        for digits, span in (("12", 3), ("1a", 1), ("123", -1)):
            with self.assertRaises(ValueError):
                largest_product(digits, span)

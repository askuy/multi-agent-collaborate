import unittest

from kit._stub import is_stub
from kit.rle import encode, decode


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (encode, decode,)):
            self.skipTest("not implemented yet")

    def test_rle(self):
        self.assertEqual(encode("aaabccdddd"), "3a1b2c4d")
        self.assertEqual(encode(""), "")
        self.assertEqual(encode("a" * 12), "12a")
        self.assertEqual(decode("3a1b2c4d"), "aaabccdddd")
        self.assertEqual(decode("12a"), "a" * 12)
        for bad in ("3", "a"):
            with self.assertRaises(ValueError):
                decode(bad)

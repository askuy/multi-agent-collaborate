import unittest

from kit._stub import is_stub
from kit.wrap import wrap


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (wrap,)):
            self.skipTest("not implemented yet")

    def test_wrap(self):
        self.assertEqual(wrap("the quick brown fox", 10), ["the quick", "brown fox"])
        self.assertEqual(wrap("", 5), [])
        self.assertEqual(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"])
        self.assertEqual(wrap("a bcdefgh", 4), ["a", "bcde", "fgh"])
        self.assertEqual(wrap("ab cdefgh i", 4), ["ab", "cdef", "gh i"])

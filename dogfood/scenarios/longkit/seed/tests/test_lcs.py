import unittest

from kit._stub import is_stub
from kit.lcs import length, sequence


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (length, sequence,)):
            self.skipTest("not implemented yet")

    def test_lcs(self):
        self.assertEqual(length("ABCBDAB", "BDCABA"), 4)
        self.assertEqual(length("", "a"), 0)
        s = sequence("ABCBDAB", "BDCABA")
        self.assertEqual(len(s), 4)
        for whole in ("ABCBDAB", "BDCABA"):
            rest = iter(whole)
            self.assertTrue(all(c in rest for c in s), s)
        self.assertEqual(sequence("abc", "xyz"), "")

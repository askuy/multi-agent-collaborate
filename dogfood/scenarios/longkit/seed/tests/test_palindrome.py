import unittest

from kit._stub import is_stub
from kit.palindrome import longest


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (longest,)):
            self.skipTest("not implemented yet")

    def test_palindrome(self):
        self.assertEqual(longest("babad"), "bab")
        self.assertEqual(longest("cbbd"), "bb")
        self.assertEqual(longest(""), "")
        self.assertEqual(longest("abc"), "a")
        self.assertEqual(longest("forgeeksskeegfor"), "geeksskeeg")

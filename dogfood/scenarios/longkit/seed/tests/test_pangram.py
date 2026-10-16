import unittest

from kit._stub import is_stub
from kit.pangram import missing


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (missing,)):
            self.skipTest("not implemented yet")

    def test_pangram(self):
        self.assertEqual(missing("The quick brown fox jumps over the lazy dog"), "")
        self.assertEqual(missing("abc"), "defghijklmnopqrstuvwxyz")
        self.assertEqual(missing(""), "abcdefghijklmnopqrstuvwxyz")

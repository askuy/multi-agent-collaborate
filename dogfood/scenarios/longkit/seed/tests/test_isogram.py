import unittest

from kit._stub import is_stub
from kit.isogram import is_isogram


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (is_isogram,)):
            self.skipTest("not implemented yet")

    def test_isogram(self):
        self.assertTrue(is_isogram("lumberjacks"))
        self.assertTrue(is_isogram("six-year-old"))
        self.assertTrue(is_isogram(""))
        self.assertFalse(is_isogram("isograms"))
        self.assertFalse(is_isogram("Alphabet"))

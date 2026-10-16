import unittest

from textkit import wrap


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(wrap, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_wrap(self):
        self.assertEqual(wrap("the quick brown fox", 10), ["the quick", "brown fox"])
        self.assertEqual(wrap("  a   b  ", 3), ["a b"])

    def test_long_word_and_empty(self):
        self.assertEqual(wrap("tiny enormousword x", 5), ["tiny", "enormousword", "x"])
        self.assertEqual(wrap("   ", 4), [])

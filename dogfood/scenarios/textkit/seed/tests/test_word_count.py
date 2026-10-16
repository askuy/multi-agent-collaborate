import unittest

from textkit import word_count


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(word_count, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_counts(self):
        self.assertEqual(word_count("It's it's IT"), {"it's": 2, "it": 1})

    def test_punctuation(self):
        self.assertEqual(word_count("a, b. A!"), {"a": 2, "b": 1})
        self.assertEqual(word_count(""), {})

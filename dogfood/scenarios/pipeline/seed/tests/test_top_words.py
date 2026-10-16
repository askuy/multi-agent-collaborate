import unittest

from pipeline._stub import is_stub
from pipeline.stats import top_words
from pipeline.text import normalize
from pipeline.tokens import tokenize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (top_words, tokenize, normalize,)):
            self.skipTest("not implemented yet (or a dependency is not)")

    def test_top_words(self):
        self.assertEqual(top_words("b a b c a b", 2), [("b", 3), ("a", 2)])
        self.assertEqual(top_words("y x", 5), [("x", 1), ("y", 1)])

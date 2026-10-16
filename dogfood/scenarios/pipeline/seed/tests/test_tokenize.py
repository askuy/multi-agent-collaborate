import unittest

from pipeline._stub import is_stub
from pipeline.text import normalize
from pipeline.tokens import tokenize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (tokenize, normalize,)):
            self.skipTest("not implemented yet (or a dependency is not)")

    def test_tokenize(self):
        self.assertEqual(tokenize("It's  ÉTÉ"), ["it's", "ete"])
        self.assertEqual(tokenize("a, b. a!"), ["a", "b", "a"])

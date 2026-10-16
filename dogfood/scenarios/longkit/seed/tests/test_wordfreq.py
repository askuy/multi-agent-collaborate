import unittest

from kit._stub import is_stub
from kit.wordfreq import top


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (top,)):
            self.skipTest("not implemented yet")

    def test_wordfreq(self):
        self.assertEqual(top("the cat and the hat and the bat", 2), [("the", 3), ("and", 2)])
        self.assertEqual(top("", 3), [])
        self.assertEqual(top("Don't stop, don't", 1), [("don't", 2)])
        self.assertEqual(top("b a", 5), [("a", 1), ("b", 1)])

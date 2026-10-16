import unittest

from kit._stub import is_stub
from kit.scrabble import score


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (score,)):
            self.skipTest("not implemented yet")

    def test_scrabble(self):
        self.assertEqual(score("cabbage"), 14)
        self.assertEqual(score("Quiz"), 22)
        self.assertEqual(score(""), 0)
        self.assertEqual(score("ab", 3), 12)
        with self.assertRaises(ValueError):
            score("a1")

import unittest

from kit._stub import is_stub
from kit.bowling import score


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (score,)):
            self.skipTest("not implemented yet")

    def test_bowling(self):
        self.assertEqual(score([0] * 20), 0)
        self.assertEqual(score([1] * 20), 20)
        self.assertEqual(score([10] * 12), 300)
        self.assertEqual(score([5, 5] * 10 + [5]), 150)
        self.assertEqual(score([10, 7, 3, 9, 0, 10, 0, 8, 8, 2, 0, 6, 10, 10, 10, 8, 1]), 167)

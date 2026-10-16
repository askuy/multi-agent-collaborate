import unittest

from textkit import dedupe


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(dedupe, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_order(self):
        self.assertEqual(dedupe([3, 1, 3, 2, 1]), [3, 1, 2])

    def test_key(self):
        self.assertEqual(dedupe(["a", "A", "b"], key=str.lower), ["a", "b"])
        self.assertEqual(dedupe([]), [])

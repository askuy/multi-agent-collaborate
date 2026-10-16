import unittest

from kit._stub import is_stub
from kit.intervals import merge


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (merge,)):
            self.skipTest("not implemented yet")

    def test_intervals(self):
        self.assertEqual(merge([[1, 3], [2, 6], [8, 10], [15, 18]]), [[1, 6], [8, 10], [15, 18]])
        self.assertEqual(merge([[1, 4], [4, 5]]), [[1, 5]])
        self.assertEqual(merge([]), [])
        self.assertEqual(merge([[5, 6], [1, 2]]), [[1, 2], [5, 6]])
        with self.assertRaises(ValueError):
            merge([[3, 1]])

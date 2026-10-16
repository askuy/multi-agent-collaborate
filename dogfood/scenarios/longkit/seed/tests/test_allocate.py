import unittest

from kit._stub import is_stub
from kit.allocate import allocate


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (allocate,)):
            self.skipTest("not implemented yet")

    def test_allocate(self):
        self.assertEqual(allocate(100, [1, 1, 1]), [34, 33, 33])
        self.assertEqual(allocate(10, [1, 2]), [3, 7])
        self.assertEqual(allocate(5, [0, 1]), [0, 5])
        for bad in ([], [0, 0]):
            with self.assertRaises(ValueError):
                allocate(5, bad)

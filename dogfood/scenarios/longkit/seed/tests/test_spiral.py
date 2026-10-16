import unittest

from kit._stub import is_stub
from kit.spiral import spiral


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (spiral,)):
            self.skipTest("not implemented yet")

    def test_spiral(self):
        self.assertEqual(spiral([[1, 2, 3], [4, 5, 6], [7, 8, 9]]), [1, 2, 3, 6, 9, 8, 7, 4, 5])
        self.assertEqual(spiral([]), [])
        self.assertEqual(spiral([[1, 2, 3, 4]]), [1, 2, 3, 4])
        self.assertEqual(spiral([[1], [2], [3]]), [1, 2, 3])
        self.assertEqual(spiral([[1, 2], [3, 4], [5, 6]]), [1, 2, 4, 6, 5, 3])

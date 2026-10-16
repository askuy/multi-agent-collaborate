import unittest

from kit._stub import is_stub
from kit.matmul import multiply


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (multiply,)):
            self.skipTest("not implemented yet")

    def test_matmul(self):
        self.assertEqual(multiply([[1, 2], [3, 4]], [[5, 6], [7, 8]]), [[19, 22], [43, 50]])
        self.assertEqual(multiply([[1, 2, 3]], [[1], [2], [3]]), [[14]])
        with self.assertRaises(ValueError):
            multiply([[1, 2]], [[1, 2]])

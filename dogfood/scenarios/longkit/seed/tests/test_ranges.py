import unittest

from kit._stub import is_stub
from kit.ranges import expand, compress


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (expand, compress,)):
            self.skipTest("not implemented yet")

    def test_ranges(self):
        self.assertEqual(expand("1-3,5,7-9"), [1, 2, 3, 5, 7, 8, 9])
        self.assertEqual(expand(""), [])
        self.assertEqual(expand("3,1-2,2"), [1, 2, 3])
        with self.assertRaises(ValueError):
            expand("5-3")
        self.assertEqual(compress([1, 2, 3, 5, 7, 8, 9]), "1-3,5,7-9")
        self.assertEqual(compress([]), "")
        self.assertEqual(compress([4, 2, 3, 3]), "2-4")
        self.assertEqual(compress([1, 2]), "1-2")

import unittest

from kit._stub import is_stub
from kit.pagination import pages


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (pages,)):
            self.skipTest("not implemented yet")

    def test_pagination(self):
        self.assertEqual(pages(10, 5, 1), [1, "...", 4, 5, 6, "...", 10])
        self.assertEqual(pages(5, 3, 1), [1, 2, 3, 4, 5])
        self.assertEqual(pages(10, 1, 1), [1, 2, "...", 10])
        self.assertEqual(pages(1, 1, 2), [1])
        self.assertEqual(pages(7, 4, 1), [1, 2, 3, 4, 5, 6, 7])
        with self.assertRaises(ValueError):
            pages(5, 6, 1)

import unittest

from kit._stub import is_stub
from kit.reorder import reorder


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (reorder,)):
            self.skipTest("not implemented yet")

    def test_reorder(self):
        self.assertEqual(reorder({"a": 10, "b": 100}, {"a": 3, "b": 1.5}, 5, 2), {"a": 11})
        self.assertEqual(reorder({}, {"c": 0.4}, 2, 1), {"c": 2})
        self.assertEqual(reorder({"d": 5}, {"d": 1}, 3, 2), {})
        self.assertEqual(reorder({"e": 0}, {}, 3, 2), {})

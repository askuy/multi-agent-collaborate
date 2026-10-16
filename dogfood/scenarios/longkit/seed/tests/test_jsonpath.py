import unittest

from kit._stub import is_stub
from kit.jsonpath import get


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (get,)):
            self.skipTest("not implemented yet")

    def test_jsonpath(self):
        self.assertEqual(get({"a": {"b": [0, 1, {"c": 5}]}}, "a.b[2].c"), 5)
        self.assertEqual(get({"a": [1, 2]}, "a[1]"), 2)
        self.assertIsNone(get({"a": 1}, "a.b"))
        self.assertIsNone(get({"a": [1]}, "a[5]"))
        self.assertEqual(get({"x": {"y": 2}}, "x.y"), 2)
        self.assertEqual(get([[1, 2], [3]], "[1][0]"), 3)

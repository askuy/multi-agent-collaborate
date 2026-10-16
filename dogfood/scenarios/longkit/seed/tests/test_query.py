import unittest

from kit._stub import is_stub
from kit.query import parse, build


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (parse, build,)):
            self.skipTest("not implemented yet")

    def test_query(self):
        self.assertEqual(parse("a=1&b=2&a=3"), {"a": ["1", "3"], "b": ["2"]})
        self.assertEqual(parse("q=hello+world%21"), {"q": ["hello world!"]})
        self.assertEqual(parse("flag"), {"flag": [""]})
        self.assertEqual(parse(""), {})
        self.assertEqual(build({"b": ["2"], "a": ["1", "x y"]}), "a=1&a=x+y&b=2")
        self.assertEqual(build({"k": ["a&b"]}), "k=a%26b")

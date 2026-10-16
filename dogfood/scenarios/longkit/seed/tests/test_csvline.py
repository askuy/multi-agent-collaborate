import unittest

from kit._stub import is_stub
from kit.csvline import parse


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (parse,)):
            self.skipTest("not implemented yet")

    def test_csvline(self):
        self.assertEqual(parse("a,b,c"), ["a", "b", "c"])
        self.assertEqual(parse('"a,b",c'), ["a,b", "c"])
        self.assertEqual(parse('"say ""hi""",x'), ['say "hi"', "x"])
        self.assertEqual(parse("a,,b"), ["a", "", "b"])
        self.assertEqual(parse(""), [""])
        self.assertEqual(parse(" a , b "), [" a ", " b "])

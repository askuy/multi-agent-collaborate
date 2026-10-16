import unittest

from kit._stub import is_stub
from kit.paths import normalize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (normalize,)):
            self.skipTest("not implemented yet")

    def test_paths(self):
        self.assertEqual(normalize("/a/./b/../c/"), "/a/c")
        self.assertEqual(normalize("a//b"), "a/b")
        self.assertEqual(normalize("/../x"), "/x")
        self.assertEqual(normalize("../a/.."), "..")
        self.assertEqual(normalize(""), ".")
        self.assertEqual(normalize("/"), "/")

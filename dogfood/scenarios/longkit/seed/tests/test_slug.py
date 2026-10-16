import unittest

from kit._stub import is_stub
from kit.slug import slugify


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (slugify,)):
            self.skipTest("not implemented yet")

    def test_slug(self):
        self.assertEqual(slugify("Héllo, Wörld!"), "hello-world")
        self.assertEqual(slugify("  --A  B--  "), "a-b")
        self.assertEqual(slugify(""), "")
        self.assertEqual(slugify("C++ & Rust"), "c-rust")

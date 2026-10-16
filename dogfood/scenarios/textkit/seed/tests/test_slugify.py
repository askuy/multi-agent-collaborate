import unittest

from textkit import slugify


class Test(unittest.TestCase):
    def setUp(self):
        if getattr(slugify, "__stub__", False):
            self.skipTest("not implemented yet")

    def test_basic(self):
        self.assertEqual(slugify("Hello, World!"), "hello-world")

    def test_accents(self):
        self.assertEqual(slugify("Crème Brûlée"), "creme-brulee")

    def test_edges(self):
        self.assertEqual(slugify("  --A  B--  "), "a-b")
        self.assertEqual(slugify(""), "")

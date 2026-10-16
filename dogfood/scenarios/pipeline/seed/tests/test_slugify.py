import unittest

from pipeline._stub import is_stub
from pipeline.slug import slugify
from pipeline.text import normalize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (slugify, normalize,)):
            self.skipTest("not implemented yet (or a dependency is not)")

    def test_slugify(self):
        self.assertEqual(slugify("Crème Brûlée, Again!"), "creme-brulee-again")
        self.assertEqual(slugify("--x--"), "x")

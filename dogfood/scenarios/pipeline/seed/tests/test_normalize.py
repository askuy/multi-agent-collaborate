import unittest

from pipeline._stub import is_stub
from pipeline.text import normalize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (normalize,)):
            self.skipTest("not implemented yet (or a dependency is not)")

    def test_normalize(self):
        self.assertEqual(normalize("Crème  Brûlée\n"), "creme brulee")
        self.assertEqual(normalize("  A\tB  "), "a b")
        self.assertEqual(normalize(""), "")

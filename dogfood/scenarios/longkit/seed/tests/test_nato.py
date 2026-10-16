import unittest

from kit._stub import is_stub
from kit.nato import spell


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (spell,)):
            self.skipTest("not implemented yet")

    def test_nato(self):
        self.assertEqual(spell("abc"), "Alfa Bravo Charlie")
        self.assertEqual(spell("Hi 2"), "Hotel India Two")
        self.assertEqual(spell("xj"), "X-ray Juliett")
        with self.assertRaises(ValueError):
            spell("a!")

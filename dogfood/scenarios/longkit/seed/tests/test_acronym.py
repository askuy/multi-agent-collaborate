import unittest

from kit._stub import is_stub
from kit.acronym import make


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (make,)):
            self.skipTest("not implemented yet")

    def test_acronym(self):
        self.assertEqual(make("Portable Network Graphics"), "PNG")
        self.assertEqual(make("Complementary metal-oxide semiconductor"), "CMOS")
        self.assertEqual(make("The Road _Not_ Taken"), "TRNT")
        self.assertEqual(make("Halley's Comet"), "HC")

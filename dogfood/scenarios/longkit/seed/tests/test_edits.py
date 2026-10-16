import unittest

from kit._stub import is_stub
from kit.edits import distance, closest


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (distance, closest,)):
            self.skipTest("not implemented yet")

    def test_edits(self):
        self.assertEqual(distance("kitten", "sitting"), 3)
        self.assertEqual(distance("", "abc"), 3)
        self.assertEqual(distance("flaw", "lawn"), 2)
        self.assertEqual(closest("appel", ["ape", "apple", "apply"]), "ape")
        self.assertEqual(closest("aple", ["maple", "apple"]), "maple")
        self.assertIsNone(closest("x", []))

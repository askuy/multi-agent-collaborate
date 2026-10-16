import unittest

from kit._stub import is_stub
from kit.grades import letter


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (letter,)):
            self.skipTest("not implemented yet")

    def test_grades(self):
        self.assertEqual(letter(95), "A")
        self.assertEqual(letter(92), "A-")
        self.assertEqual(letter(98), "A")
        self.assertEqual(letter(100), "A")
        self.assertEqual(letter(88), "B+")
        self.assertEqual(letter(80), "B-")
        self.assertEqual(letter(75), "C")
        self.assertEqual(letter(59), "F")
        with self.assertRaises(ValueError):
            letter(101)

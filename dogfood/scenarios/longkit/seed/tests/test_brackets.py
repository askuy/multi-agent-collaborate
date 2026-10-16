import unittest

from kit._stub import is_stub
from kit.brackets import balanced


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (balanced,)):
            self.skipTest("not implemented yet")

    def test_brackets(self):
        self.assertTrue(balanced("{[()]}"))
        self.assertFalse(balanced("([)]"))
        self.assertTrue(balanced(""))
        self.assertFalse(balanced("((("))
        self.assertFalse(balanced("a(b)c]"))

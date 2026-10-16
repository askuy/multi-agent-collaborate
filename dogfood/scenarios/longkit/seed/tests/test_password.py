import unittest

from kit._stub import is_stub
from kit.password import strength


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (strength,)):
            self.skipTest("not implemented yet")

    def test_password(self):
        self.assertEqual(strength(""), 0)
        self.assertEqual(strength("abc"), 1)
        self.assertEqual(strength("Abcdefg1!"), 5)
        self.assertEqual(strength("PASSWORD"), 0)
        self.assertEqual(strength("abcdefgh"), 2)

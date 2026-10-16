import unittest

from kit._stub import is_stub
from kit.caesar import shift


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (shift,)):
            self.skipTest("not implemented yet")

    def test_caesar(self):
        self.assertEqual(shift("abc", 1), "bcd")
        self.assertEqual(shift("xyz", 3), "abc")
        self.assertEqual(shift("Hello, World!", 13), "Uryyb, Jbeyq!")
        self.assertEqual(shift("abc", -1), "zab")
        self.assertEqual(shift("abc", 27), "bcd")

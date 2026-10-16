import unittest

from kit._stub import is_stub
from kit.baseconv import to_base, from_base


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (to_base, from_base,)):
            self.skipTest("not implemented yet")

    def test_baseconv(self):
        self.assertEqual(to_base(255, 16), "ff")
        self.assertEqual(to_base(0, 2), "0")
        self.assertEqual(to_base(-10, 2), "-1010")
        self.assertEqual(from_base("ff", 16), 255)
        self.assertEqual(from_base("Z", 36), 35)
        with self.assertRaises(ValueError):
            from_base("2", 2)
        with self.assertRaises(ValueError):
            to_base(5, 1)

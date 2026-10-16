import unittest

from kit._stub import is_stub
from kit.luhn import valid, check_digit


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (valid, check_digit,)):
            self.skipTest("not implemented yet")

    def test_luhn(self):
        self.assertTrue(valid("4539 1488 0343 6467"))
        self.assertFalse(valid("8273 1232 7352 0569"))
        self.assertFalse(valid("0"))
        self.assertFalse(valid("059a"))
        self.assertTrue(valid("79927398713"))
        self.assertEqual(check_digit("7992739871"), "3")

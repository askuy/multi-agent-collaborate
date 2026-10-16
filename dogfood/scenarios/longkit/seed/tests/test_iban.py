import unittest

from kit._stub import is_stub
from kit.iban import valid


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (valid,)):
            self.skipTest("not implemented yet")

    def test_iban(self):
        self.assertTrue(valid("GB82 WEST 1234 5698 7654 32"))
        self.assertFalse(valid("GB82 TEST 1234 5698 7654 32"))
        self.assertTrue(valid("DE89 3704 0044 0532 0130 00"))
        self.assertFalse(valid("XX"))
        self.assertFalse(valid("GB82 WEST 1234 5698 7654 3!"))

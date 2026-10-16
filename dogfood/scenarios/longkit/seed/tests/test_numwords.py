import unittest

from kit._stub import is_stub
from kit.numwords import say


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (say,)):
            self.skipTest("not implemented yet")

    def test_numwords(self):
        self.assertEqual(say(0), "zero")
        self.assertEqual(say(13), "thirteen")
        self.assertEqual(say(21), "twenty-one")
        self.assertEqual(say(100), "one hundred")
        self.assertEqual(say(123), "one hundred twenty-three")
        self.assertEqual(say(1005), "one thousand five")
        self.assertEqual(say(999999), "nine hundred ninety-nine thousand nine hundred ninety-nine")
        with self.assertRaises(ValueError):
            say(1000000)

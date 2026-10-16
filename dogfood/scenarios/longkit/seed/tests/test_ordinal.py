import unittest

from kit._stub import is_stub
from kit.ordinal import ordinal


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (ordinal,)):
            self.skipTest("not implemented yet")

    def test_ordinal(self):
        for n, text in [(0, "0th"), (1, "1st"), (2, "2nd"), (3, "3rd"), (4, "4th"), (11, "11th"),
                        (12, "12th"), (13, "13th"), (21, "21st"), (102, "102nd"), (111, "111th")]:
            self.assertEqual(ordinal(n), text)
        with self.assertRaises(ValueError):
            ordinal(-1)

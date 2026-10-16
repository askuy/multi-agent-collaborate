import unittest

from kit._stub import is_stub
from kit.phone import format


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (format,)):
            self.skipTest("not implemented yet")

    def test_phone(self):
        self.assertEqual(format("613-995-0253"), "(613) 995-0253")
        self.assertEqual(format("+1 (613) 995-0253"), "(613) 995-0253")
        self.assertEqual(format("613.995.0253"), "(613) 995-0253")
        for bad in ("123-456-7890", "613-095-0253", "22-995-0253", "613-995-025a"):
            with self.assertRaises(ValueError):
                format(bad)

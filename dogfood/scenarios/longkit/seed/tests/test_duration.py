import unittest

from kit._stub import is_stub
from kit.duration import parse, format


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (parse, format,)):
            self.skipTest("not implemented yet")

    def test_duration(self):
        self.assertEqual(parse("1h30m"), 5400)
        self.assertEqual(parse("45s"), 45)
        self.assertEqual(parse("2d"), 172800)
        for bad in ("", "1x", "30m1h", "1h1h", "1 h"):
            with self.assertRaises(ValueError):
                parse(bad)
        self.assertEqual(format(5400), "1h30m")
        self.assertEqual(format(0), "0s")
        self.assertEqual(format(90061), "1d1h1m1s")

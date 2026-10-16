import unittest

from kit._stub import is_stub
from kit.bytesize import format, parse


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (format, parse,)):
            self.skipTest("not implemented yet")

    def test_bytesize(self):
        self.assertEqual(format(0), "0 B")
        self.assertEqual(format(1023), "1023 B")
        self.assertEqual(format(1536), "1.5 KiB")
        self.assertEqual(format(1048576), "1 MiB")
        self.assertEqual(format(1100), "1.1 KiB")
        self.assertEqual(parse("1.5 KiB"), 1536)
        self.assertEqual(parse("2 MiB"), 2097152)
        self.assertEqual(parse("10 B"), 10)
        with self.assertRaises(ValueError):
            parse("1 XB")

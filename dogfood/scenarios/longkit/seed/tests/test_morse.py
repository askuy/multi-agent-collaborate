import unittest

from kit._stub import is_stub
from kit.morse import encode, decode


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (encode, decode,)):
            self.skipTest("not implemented yet")

    def test_morse(self):
        self.assertEqual(encode("SOS"), "... --- ...")
        self.assertEqual(encode("hi there"), ".... .. / - .... . .-. .")
        self.assertEqual(encode("a1"), ".- .----")
        self.assertEqual(decode("... --- ..."), "SOS")
        self.assertEqual(decode(".... .. / - .... . .-. ."), "HI THERE")
        with self.assertRaises(ValueError):
            encode("@")

import unittest

from kit._stub import is_stub
from kit.isbn import valid, to_isbn13


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (valid, to_isbn13,)):
            self.skipTest("not implemented yet")

    def test_isbn(self):
        self.assertTrue(valid("0-306-40615-2"))
        self.assertTrue(valid("978-0-306-40615-7"))
        self.assertFalse(valid("0-306-40615-3"))
        self.assertTrue(valid("080442957X"))
        self.assertFalse(valid("123"))
        self.assertEqual(to_isbn13("0-306-40615-2"), "9780306406157")
        with self.assertRaises(ValueError):
            to_isbn13("0-306-40615-3")

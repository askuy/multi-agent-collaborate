import unittest

from kit._stub import is_stub
from kit.email import normalize


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (normalize,)):
            self.skipTest("not implemented yet")

    def test_email(self):
        self.assertEqual(normalize(" John.Doe+news@GMAIL.com "), "johndoe@gmail.com")
        self.assertEqual(normalize("a.b+c@example.com"), "a.b+c@example.com")
        self.assertEqual(normalize("x@googlemail.com"), "x@gmail.com")
        for bad in ("nope", "a@b@c", "@x.com", "x@"):
            with self.assertRaises(ValueError):
                normalize(bad)

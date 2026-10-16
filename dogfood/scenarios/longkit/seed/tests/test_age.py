import unittest

from kit._stub import is_stub
from kit.age import age


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (age,)):
            self.skipTest("not implemented yet")

    def test_age(self):
        self.assertEqual(age("2000-05-15", "2024-05-14"), 23)
        self.assertEqual(age("2000-05-15", "2024-05-15"), 24)
        self.assertEqual(age("2004-02-29", "2023-02-28"), 19)
        self.assertEqual(age("2004-02-29", "2023-02-27"), 18)
        with self.assertRaises(ValueError):
            age("2024-01-01", "2023-01-01")

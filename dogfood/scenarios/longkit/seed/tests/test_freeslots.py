import unittest

from kit._stub import is_stub
from kit.freeslots import free


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (free,)):
            self.skipTest("not implemented yet")

    def test_freeslots(self):
        self.assertEqual(
            free(["09:00-10:00", "12:00-13:00"], "08:00", "17:00", 30),
            ["08:00-09:00", "10:00-12:00", "13:00-17:00"],
        )
        self.assertEqual(free(["09:00-10:30", "10:00-11:00"], "09:00", "12:00", 15), ["11:00-12:00"])
        self.assertEqual(free([], "09:00", "10:00", 60), ["09:00-10:00"])
        self.assertEqual(free(["09:10-09:50"], "09:00", "10:00", 15), [])

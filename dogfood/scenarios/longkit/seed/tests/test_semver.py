import unittest

from kit._stub import is_stub
from kit.semver import compare


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (compare,)):
            self.skipTest("not implemented yet")

    def test_semver(self):
        self.assertEqual(compare("1.0.0", "2.0.0"), -1)
        self.assertEqual(compare("1.10.0", "1.9.0"), 1)
        self.assertEqual(compare("1.2.3", "1.2.3"), 0)
        self.assertEqual(compare("1.0.0-alpha", "1.0.0"), -1)
        self.assertEqual(compare("1.0.0-alpha.1", "1.0.0-alpha"), 1)
        self.assertEqual(compare("1.0.0-alpha.beta", "1.0.0-alpha.1"), 1)
        self.assertEqual(compare("1.0.0-2", "1.0.0-10"), -1)

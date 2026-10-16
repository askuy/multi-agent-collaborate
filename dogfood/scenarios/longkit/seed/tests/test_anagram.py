import unittest

from kit._stub import is_stub
from kit.anagram import groups


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (groups,)):
            self.skipTest("not implemented yet")

    def test_anagram(self):
        self.assertEqual(
            groups(["tea", "eat", "tan", "ate", "nat", "bat"]),
            [["tea", "eat", "ate"], ["tan", "nat"], ["bat"]],
        )
        self.assertEqual(groups([]), [])
        self.assertEqual(groups(["Listen", "silent"]), [["Listen", "silent"]])

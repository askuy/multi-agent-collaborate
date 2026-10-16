import unittest

from kit._stub import is_stub
from kit.table import render


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (render,)):
            self.skipTest("not implemented yet")

    def test_table(self):
        self.assertEqual(
            render(["name", "qty"], [["apple", 3], ["kiwi", 12]]),
            "| name  | qty |\n| ----- | --- |\n| apple | 3   |\n| kiwi  | 12  |\n",
        )
        self.assertEqual(render(["a"], []), "| a |\n| - |\n")

import unittest

from kit._stub import is_stub
from kit.histogram import render


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (render,)):
            self.skipTest("not implemented yet")

    def test_histogram(self):
        self.assertEqual(render({"b": 2, "a": 4}, 8), "a | ######## 4\nb | #### 2\n")
        self.assertEqual(render({"x": 1, "long": 3}, 6), "long | ###### 3\nx    | ## 1\n")
        self.assertEqual(render({}, 5), "")
        self.assertEqual(render({"z": 0, "y": 5}, 5), "y | ##### 5\nz |  0\n")

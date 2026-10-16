import unittest

from kit._stub import is_stub
from kit.income import tax


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (tax,)):
            self.skipTest("not implemented yet")

    def test_income(self):
        brackets = [(10000, 0), (40000, 20), (None, 40)]
        self.assertEqual(tax(5000, brackets), 0)
        self.assertEqual(tax(10000, brackets), 0)
        self.assertEqual(tax(25000, brackets), 3000.0)
        self.assertEqual(tax(50000, brackets), 10000.0)
        with self.assertRaises(ValueError):
            tax(-1, brackets)

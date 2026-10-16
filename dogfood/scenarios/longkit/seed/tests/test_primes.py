import unittest

from kit._stub import is_stub
from kit.primes import factorize, primes_up_to


class Test(unittest.TestCase):
    def setUp(self):
        if any(is_stub(f) for f in (factorize, primes_up_to,)):
            self.skipTest("not implemented yet")

    def test_primes(self):
        self.assertEqual(factorize(60), [2, 2, 3, 5])
        self.assertEqual(factorize(97), [97])
        with self.assertRaises(ValueError):
            factorize(1)
        self.assertEqual(primes_up_to(20), [2, 3, 5, 7, 11, 13, 17, 19])
        self.assertEqual(primes_up_to(1), [])

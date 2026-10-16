from kit._stub import stub


@stub
def factorize(n: int) -> list:
    """Prime factors of n >= 2 in ascending order, with multiplicity; ValueError for n < 2."""
    raise NotImplementedError


@stub
def primes_up_to(n: int) -> list:
    """All primes <= n in ascending order."""
    raise NotImplementedError

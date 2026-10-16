from kit._stub import stub


@stub
def payment(principal: float, annual_rate: float, months: int) -> float:
    """Monthly payment of an amortized loan (annual rate in percent, compounded monthly), rounded to 2 decimals; with a 0 rate, principal / months. ValueError if months < 1."""
    raise NotImplementedError

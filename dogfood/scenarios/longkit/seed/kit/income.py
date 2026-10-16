from kit._stub import stub


@stub
def tax(income: float, brackets: list) -> float:
    """Progressive tax: `brackets` is a list of (upper limit or None for the top bracket, rate in percent), ascending; each slice of income is taxed at its bracket's rate. Rounded to 2 decimals. ValueError for negative income."""
    raise NotImplementedError

from kit._stub import stub


@stub
def allocate(total: int, ratios: list) -> list:
    """Split an integer total in proportion to `ratios`: everyone gets the floor of their exact share, and the units left over go one each to the largest fractional remainders (the earliest on ties). ValueError if there are no ratios or they sum to 0."""
    raise NotImplementedError

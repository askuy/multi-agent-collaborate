from kit._stub import stub


@stub
def cost(weight_kg: float, zone: str) -> float:
    """Shipping cost: zone 'domestic', 'eu', or 'world' costs 5.00, 9.00, or 15.00 for the first kg and 1.50, 2.50, or 4.00 for every started kg after it. Rounded to 2 decimals. ValueError for an unknown zone or a weight not above 0 and up to 30."""
    raise NotImplementedError

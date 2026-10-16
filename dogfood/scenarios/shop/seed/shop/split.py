from shop._stub import stub


@stub
def split_bill(total: str, people: int) -> list:
    """Split `total` into `people` shares that differ by at most a cent and add up to the total;
    the first shares get the extra cents. Parse and format with shop.money."""
    raise NotImplementedError

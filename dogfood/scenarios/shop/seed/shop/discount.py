from shop._stub import stub


@stub
def apply_discount(amount: str, percent: int) -> str:
    """The amount minus `percent` percent, the discount rounded half up to the cent.
    Parse and format with shop.money."""
    raise NotImplementedError

from shop._stub import stub


@stub
def add_tax(amount: str, percent: int) -> str:
    """The amount plus `percent` percent tax, the tax rounded half up to the cent.
    Parse and format with shop.money."""
    raise NotImplementedError

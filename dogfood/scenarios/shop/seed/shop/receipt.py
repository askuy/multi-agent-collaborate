from shop._stub import stub


@stub
def render(items: list, percent: int) -> str:
    """A receipt for (name, amount) items: one line `<name> <amount>` per item, then
    `subtotal <sum>` and `total <sum with tax>`, each line ending in a newline. The total is
    shop.tax.add_tax(subtotal, percent). Parse and format with shop.money."""
    raise NotImplementedError

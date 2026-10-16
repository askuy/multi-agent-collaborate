from textkit._stub import stub


@stub
def dedupe(items: list, key=None) -> list:
    """Items in their original order without later duplicates. With key, two items are duplicates when key(item) is equal; the first one is kept."""
    raise NotImplementedError

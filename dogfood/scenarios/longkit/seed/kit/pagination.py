from kit._stub import stub


@stub
def pages(total: int, current: int, window: int) -> list:
    """Page links to show: the first and last page, the pages within `window` of `current`, and '...' for a gap of two or more pages (a gap of one page shows that page). ValueError if `current` is not between 1 and `total`."""
    raise NotImplementedError

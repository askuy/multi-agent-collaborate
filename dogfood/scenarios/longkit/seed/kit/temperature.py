from kit._stub import stub


@stub
def convert(value: float, src: str, dst: str) -> float:
    """Convert between 'C', 'F', and 'K', rounded to 2 decimals. ValueError for another unit or a temperature below absolute zero."""
    raise NotImplementedError

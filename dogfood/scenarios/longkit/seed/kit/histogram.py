from kit._stub import stub


@stub
def render(counts: dict, width: int) -> str:
    """One line per key in sorted order: '<key padded to the longest key> | <bar> <count>', every line ending in a newline. The largest count gets `width` '#'; the others are scaled and rounded half up."""
    raise NotImplementedError

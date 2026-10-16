from kit._stub import stub


@stub
def expand(spec: str) -> list:
    """Numbers in a spec like '1-3,5,7-9', sorted and without duplicates; '' is empty. A range whose start is above its end is a ValueError."""
    raise NotImplementedError


@stub
def compress(numbers: list) -> str:
    """The shortest spec for a list of integers (any order, duplicates allowed)."""
    raise NotImplementedError

from kit._stub import stub


@stub
def to_roman(n: int) -> str:
    """Roman numeral for 1 <= n <= 3999; ValueError otherwise."""
    raise NotImplementedError


@stub
def from_roman(s: str) -> int:
    """Value of a numeral in canonical form (as to_roman writes it); ValueError otherwise."""
    raise NotImplementedError

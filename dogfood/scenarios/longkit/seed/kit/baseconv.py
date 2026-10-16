from kit._stub import stub


@stub
def to_base(n: int, base: int) -> str:
    """n in a base from 2 to 36 (digits 0-9 then a-z, a leading '-' if negative); ValueError for another base."""
    raise NotImplementedError


@stub
def from_base(text: str, base: int) -> int:
    """The inverse (letters in either case); ValueError for an invalid digit or base."""
    raise NotImplementedError

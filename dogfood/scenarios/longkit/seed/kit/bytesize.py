from kit._stub import stub


@stub
def format(n: int) -> str:
    """Below 1024: '<n> B'. Otherwise in KiB, MiB, GiB, or TiB (the largest unit with a value of at least 1) with one decimal, a trailing '.0' dropped: '1.5 KiB', '1 MiB'."""
    raise NotImplementedError


@stub
def parse(text: str) -> int:
    """The inverse: '<number> <unit>' with unit B, KiB, MiB, GiB, or TiB; ValueError otherwise."""
    raise NotImplementedError

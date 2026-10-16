from kit._stub import stub


@stub
def parse(text: str) -> int:
    """Seconds in a duration like '1d2h3m4s': units d, h, m, s, in that order, each at most once, no spaces, at least one. ValueError otherwise."""
    raise NotImplementedError


@stub
def format(seconds: int) -> str:
    """The shortest such text for `seconds` ('0s' for 0)."""
    raise NotImplementedError

from textkit._stub import stub


@stub
def parse_duration(text: str) -> int:
    """Seconds in a duration like '1d2h30m15s' or '2h 5m' (units d, h, m, s, each at most once, in that order, whitespace allowed between parts). Raises ValueError for empty or malformed input."""
    raise NotImplementedError

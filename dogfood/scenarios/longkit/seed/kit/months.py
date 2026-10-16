from kit._stub import stub


@stub
def is_leap(year: int) -> bool:
    """Gregorian leap year."""
    raise NotImplementedError


@stub
def days_in_month(year: int, month: int) -> int:
    """Days in the month; ValueError for a month outside 1-12."""
    raise NotImplementedError

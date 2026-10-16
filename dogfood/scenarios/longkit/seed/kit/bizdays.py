from kit._stub import stub


@stub
def add_business_days(date: str, n: int) -> str:
    """The ISO date (YYYY-MM-DD) `n` business days (Monday to Friday) after `date`, or before it if `n` is negative; n = 0 returns `date` unchanged. Counting starts from the next day, so a weekend start works."""
    raise NotImplementedError

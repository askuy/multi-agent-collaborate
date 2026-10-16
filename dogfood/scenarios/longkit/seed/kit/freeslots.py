from kit._stub import stub


@stub
def free(busy: list, start: str, end: str, min_minutes: int) -> list:
    """Free slots of at least `min_minutes` between `start` and `end` ('HH:MM'), given busy slots 'HH:MM-HH:MM' in any order and possibly overlapping, as 'HH:MM-HH:MM' strings in order."""
    raise NotImplementedError

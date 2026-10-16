from kit._stub import stub


@stub
def to_24h(text: str) -> str:
    """'1:05 PM' -> '13:05', '12:00 AM' -> '00:00'; ValueError for an invalid time."""
    raise NotImplementedError


@stub
def to_12h(text: str) -> str:
    """'00:30' -> '12:30 AM', '23:59' -> '11:59 PM' (hour without a leading zero); ValueError for an invalid time."""
    raise NotImplementedError

from kit._stub import stub


@stub
def valid(number: str) -> bool:
    """Whether a number passes the Luhn check. Spaces are ignored; any other non-digit, or fewer than two digits, makes it invalid."""
    raise NotImplementedError


@stub
def check_digit(partial: str) -> str:
    """The digit that, appended to `partial`, makes it pass the Luhn check."""
    raise NotImplementedError

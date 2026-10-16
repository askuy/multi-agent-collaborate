from kit._stub import stub


@stub
def say(n: int) -> str:
    """English words for 0 to 999999, tens and units joined by a hyphen, no 'and' ('one hundred twenty-three'). ValueError outside that range."""
    raise NotImplementedError

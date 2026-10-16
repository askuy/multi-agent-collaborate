from kit._stub import stub


@stub
def wrap(text: str, width: int) -> list:
    """Greedy word wrap into lines of at most `width` characters, words separated by single spaces. A word longer than `width` is cut into pieces of `width` characters (the last may be shorter), each placed like a word."""
    raise NotImplementedError

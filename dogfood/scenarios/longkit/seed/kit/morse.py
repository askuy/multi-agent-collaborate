from kit._stub import stub


@stub
def encode(text: str) -> str:
    """Morse code for letters and digits (case ignored): letters separated by a space, words by ' / '. ValueError for any other character."""
    raise NotImplementedError


@stub
def decode(code: str) -> str:
    """The inverse, in uppercase; ValueError for an unknown code."""
    raise NotImplementedError

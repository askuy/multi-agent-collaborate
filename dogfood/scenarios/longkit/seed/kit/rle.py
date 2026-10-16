from kit._stub import stub


@stub
def encode(text: str) -> str:
    """Run-length encoding: every run as <count><char>, e.g. 'aaab' -> '3a1b'."""
    raise NotImplementedError


@stub
def decode(data: str) -> str:
    """The inverse of encode; ValueError if a count or a character is missing."""
    raise NotImplementedError

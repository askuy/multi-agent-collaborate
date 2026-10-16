from kit._stub import stub


@stub
def parse(qs: str) -> dict:
    """A query string as name -> list of values, in order: percent-decoding, '+' as space, a name without '=' has the value ''; '' gives {}."""
    raise NotImplementedError


@stub
def build(params: dict) -> str:
    """The inverse: names sorted, each value in order, spaces as '+', other reserved characters percent-encoded."""
    raise NotImplementedError

from kit._stub import stub


@stub
def next_id(current: str) -> str:
    """Increment the number at the end of an id, keeping its zero padding and growing it when needed ('INV-0099' -> 'INV-0100', 'INV-9999' -> 'INV-10000'). ValueError if it does not end in a digit."""
    raise NotImplementedError

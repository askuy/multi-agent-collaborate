from kit._stub import stub


@stub
def normalize(address: str) -> str:
    """Trim and lowercase an address; for gmail.com and googlemail.com, drop dots and anything from a '+' in the local part and use gmail.com. ValueError unless there is exactly one '@' with text on both sides."""
    raise NotImplementedError

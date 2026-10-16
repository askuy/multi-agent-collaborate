from kit._stub import stub


@stub
def format(number: str) -> str:
    """A North American number as '(NXX) NXX-XXXX'. Spaces, dots, dashes, parentheses, and a leading '+' are ignored; a leading country code 1 is dropped; then exactly 10 digits must remain, and the area code and exchange must start with 2-9. ValueError otherwise."""
    raise NotImplementedError

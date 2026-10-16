from kit._stub import stub


@stub
def valid(isbn: str) -> bool:
    """Whether an ISBN-10 (last character may be X) or ISBN-13 is valid; hyphens are ignored."""
    raise NotImplementedError


@stub
def to_isbn13(isbn10: str) -> str:
    """The ISBN-13 (digits only) for a valid ISBN-10; ValueError otherwise."""
    raise NotImplementedError

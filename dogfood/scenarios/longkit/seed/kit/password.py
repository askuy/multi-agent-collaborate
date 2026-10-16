from kit._stub import stub


@stub
def strength(password: str) -> int:
    """One point each for: length >= 8, a lowercase letter, an uppercase letter, a digit, any other character. A common password ('password', '123456', 'qwerty', compared ignoring case) scores 0."""
    raise NotImplementedError

from textkit._stub import stub


@stub
def wrap(text: str, width: int) -> list:
    """Greedy word wrap: words separated by single spaces, lines at most width characters. Whitespace in the input is normalized; a word longer than width gets its own line, unbroken. Empty or blank text gives []."""
    raise NotImplementedError

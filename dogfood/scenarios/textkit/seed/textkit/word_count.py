from textkit._stub import stub


@stub
def word_count(text: str) -> dict:
    """Counts words case-insensitively. A word is a run of letters, digits, or apostrophes; keys are lowercase ('It's it's IT' -> {"it's": 2, 'it': 1})."""
    raise NotImplementedError

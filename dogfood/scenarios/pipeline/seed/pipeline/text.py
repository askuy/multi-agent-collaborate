from pipeline._stub import stub


@stub
def normalize(text: str) -> str:
    """Lowercase; accents removed (NFKD, drop combining marks); every run of whitespace becomes
    one space; no leading or trailing space. 'Crème  Brûlée\n' -> 'creme brulee'."""
    raise NotImplementedError

from textkit._stub import stub


@stub
def slugify(text: str) -> str:
    """Lowercase ASCII slug. Letters and digits are kept; every other run of characters becomes a single '-'; no leading or trailing '-'. Accented Latin letters lose their accents ('Crème Brûlée' -> 'creme-brulee')."""
    raise NotImplementedError

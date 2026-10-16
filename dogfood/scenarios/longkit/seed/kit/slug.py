from kit._stub import stub


@stub
def slugify(text: str) -> str:
    """Lowercase ASCII slug: accents removed (NFKD), every run of other characters replaced by one '-', no '-' at either end."""
    raise NotImplementedError

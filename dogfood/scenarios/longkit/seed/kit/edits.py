from kit._stub import stub


@stub
def distance(a: str, b: str) -> int:
    """Levenshtein distance (insertions, deletions, substitutions)."""
    raise NotImplementedError


@stub
def closest(word: str, candidates: list) -> object:
    """The candidate nearest to `word` by distance, the earliest one on ties; None if there are none."""
    raise NotImplementedError

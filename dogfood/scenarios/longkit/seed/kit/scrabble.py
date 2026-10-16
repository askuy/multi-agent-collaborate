from kit._stub import stub


@stub
def score(word: str, word_multiplier: int = 1) -> int:
    """Scrabble score (case ignored): AEIOULNRST 1, DG 2, BCMP 3, FHVWY 4, K 5, JX 8, QZ 10, times `word_multiplier`. ValueError for a non-letter."""
    raise NotImplementedError

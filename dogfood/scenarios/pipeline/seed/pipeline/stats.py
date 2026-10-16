from pipeline._stub import stub


@stub
def top_words(text: str, n: int) -> list:
    """The n most frequent words of tokenize(text) (use pipeline.tokens.tokenize) as
    (word, count) pairs, most frequent first, ties broken alphabetically."""
    raise NotImplementedError

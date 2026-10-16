from pipeline._stub import stub


@stub
def tokenize(text: str) -> list:
    """Words of normalize(text) (use pipeline.text.normalize): runs of a-z, 0-9, and
    apostrophes, in order. "It's  ÉTÉ" -> ["it's", 'ete']."""
    raise NotImplementedError

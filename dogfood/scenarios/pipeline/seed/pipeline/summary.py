from pipeline._stub import stub


@stub
def summarize(text: str) -> dict:
    """{'slug': slugify of the first line, 'words': number of tokens in the whole text,
    'top': top_words(text, 3)}. Use pipeline.slug.slugify, pipeline.tokens.tokenize, and
    pipeline.stats.top_words; do not reimplement them. An empty text gives
    {'slug': '', 'words': 0, 'top': []}."""
    raise NotImplementedError

from pipeline._stub import stub


@stub
def slugify(text: str) -> str:
    """Slug of normalize(text) (use pipeline.text.normalize, do not reimplement it): every run
    of characters other than a-z and 0-9 becomes one '-', no leading or trailing '-'.
    'Crème Brûlée, Again!' -> 'creme-brulee-again'."""
    raise NotImplementedError

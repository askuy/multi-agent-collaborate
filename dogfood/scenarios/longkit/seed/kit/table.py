from kit._stub import stub


@stub
def render(headers: list, rows: list) -> str:
    """A Markdown table: every column padded (left-aligned) to its widest cell, cells converted with str, a separator row of dashes as wide as each column, every line ending in a newline."""
    raise NotImplementedError

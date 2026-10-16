from kit._stub import stub


@stub
def parse(line: str) -> list:
    """Split one CSV line on commas. A field may be wrapped in double quotes, inside which commas are kept and a doubled quote stands for one quote. Whitespace is kept. An empty line is one empty field."""
    raise NotImplementedError

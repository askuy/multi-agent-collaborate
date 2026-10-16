from kit._stub import stub


@stub
def get(obj: object, path: str) -> object:
    """The value at a path like 'a.b[2].c' (keys separated by dots, list indexes in brackets, a path may start with an index); None if any step is missing."""
    raise NotImplementedError

def stub(fn):
    """Marks a function as not implemented yet; tests that need it are skipped until it is replaced."""
    fn.__stub__ = True
    return fn


def is_stub(fn) -> bool:
    return getattr(fn, "__stub__", False)

def stub(fn):
    """Marks a function as not implemented yet; its tests are skipped until it is replaced."""
    fn.__stub__ = True
    return fn

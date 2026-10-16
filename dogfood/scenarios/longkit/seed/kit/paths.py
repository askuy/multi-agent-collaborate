from kit._stub import stub


@stub
def normalize(path: str) -> str:
    """Normalize a POSIX path: collapse repeated slashes, drop '.', resolve '..' (at the root of an absolute path it is dropped; at the start of a relative path it is kept), no trailing slash except for '/'; '' becomes '.'."""
    raise NotImplementedError

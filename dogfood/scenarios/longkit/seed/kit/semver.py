from kit._stub import stub


@stub
def compare(a: str, b: str) -> int:
    """Compare two semantic versions MAJOR.MINOR.PATCH[-PRERELEASE]: -1, 0, or 1. A prerelease sorts before its release; prerelease identifiers (dot-separated) compare numerically when both are digits, otherwise as text, and numeric ones sort before alphanumeric ones; if all shared identifiers are equal, the shorter list sorts first."""
    raise NotImplementedError

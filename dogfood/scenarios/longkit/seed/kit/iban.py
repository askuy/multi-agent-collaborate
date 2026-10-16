from kit._stub import stub


@stub
def valid(iban: str) -> bool:
    """Whether an IBAN is valid: spaces ignored, two letters, two digits, then letters and digits, 15 to 34 characters in all, and the mod-97 check (rearranged, letters as 10-35) equals 1."""
    raise NotImplementedError

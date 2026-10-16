from kit._stub import stub


@stub
def letter(score: int) -> str:
    """Letter grade for 0-100: A from 90, B from 80, C from 70, D from 60, otherwise F. For A to D, a last digit of 7-9 adds '+' and 0-2 adds '-', except that there is no A+ and 100 is 'A'; F has no sign. ValueError outside 0-100."""
    raise NotImplementedError

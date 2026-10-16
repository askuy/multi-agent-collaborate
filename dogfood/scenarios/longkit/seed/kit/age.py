from kit._stub import stub


@stub
def age(birth: str, on: str) -> int:
    """Completed years between two ISO dates; a 29 February birthday falls on 28 February in non-leap years. ValueError if `on` is before `birth`."""
    raise NotImplementedError

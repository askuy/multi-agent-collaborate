from kit._stub import stub


@stub
def hex_to_rgb(text: str) -> tuple:
    """'#rrggbb' or '#rgb' (either case) to an (r, g, b) tuple; ValueError otherwise."""
    raise NotImplementedError


@stub
def rgb_to_hex(r: int, g: int, b: int) -> str:
    """Lowercase '#rrggbb'; ValueError for a value outside 0-255."""
    raise NotImplementedError

"""Parsing and formatting amounts. Every other module goes through these functions."""

API_VERSION = 1


def parse_amount(text: str) -> float:
    """'12.34' -> 12.34"""
    return float(text)


def format_amount(amount: float) -> str:
    """12.3 -> '12.30'"""
    return f"{amount:.2f}"

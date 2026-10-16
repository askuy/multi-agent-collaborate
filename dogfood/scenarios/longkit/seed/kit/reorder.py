from kit._stub import stub


@stub
def reorder(stock: dict, sales_per_day: dict, lead_days: int, safety_days: int) -> dict:
    """Quantities to order per item so that stock covers (lead_days + safety_days) of sales: ceil(sales_per_day * (lead_days + safety_days)) - stock, items missing from stock count as 0, items needing nothing left out."""
    raise NotImplementedError

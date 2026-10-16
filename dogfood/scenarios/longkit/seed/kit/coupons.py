from kit._stub import stub


@stub
def apply(subtotal: int, coupons: list) -> int:
    """Price in cents after coupons: of the percentage coupons ('10%') only the largest applies, first, its discount rounded half up to the cent; then every fixed coupon (a number of cents, '500') is subtracted; never below 0. ValueError for any other coupon."""
    raise NotImplementedError

"""The price of an order: discounts, the coupon, shipping and rounding (rules in README.md)."""
from dataclasses import dataclass
from decimal import ROUND_HALF_UP, Decimal
from typing import Optional

CENT = Decimal("0.01")


@dataclass
class Order:
    unit_price: Decimal
    quantity: int
    member: bool = False
    coupon: Optional[str] = None


def price(order: Order) -> Decimal:
    if not 1 <= order.quantity <= 999:
        raise ValueError("quantity must be 1 to 999")
    if order.unit_price < 0:
        raise ValueError("unit price must not be negative")
    subtotal = order.unit_price * order.quantity
    rate = Decimal("0")
    if order.quantity > 10:
        rate = Decimal("0.15")
    elif order.member:
        rate = Decimal("0.10")
    amount = subtotal * (1 - rate)
    shipping = Decimal("0") if subtotal >= 100 else Decimal("7")
    if order.coupon is not None:
        if order.coupon != "SAVE5":
            raise ValueError(f"unknown coupon {order.coupon}")
        if amount >= 50:
            amount -= 5
    return (amount + shipping).quantize(CENT, rounding=ROUND_HALF_UP)

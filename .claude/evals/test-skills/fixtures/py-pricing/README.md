# py-pricing

Prices an order in the shop's checkout. Python 3.10, standard library only.

Run the tests: `python -m unittest discover -s tests -t .`

Line coverage of `pricing/` is 100%.

## Rules

1. The subtotal is the unit price times the quantity. The quantity must be 1 to 999 and the unit price must not be
   negative; anything else raises `ValueError`.
2. Discount: members get 10% off. An order of 10 or more items gets 15% off. A member ordering 10 or more items gets
   only the larger discount (15%), never both.
3. Coupon: `"SAVE5"` takes 5.00 off when the amount after the discount is at least 50.00, and does nothing below
   that. Any other coupon raises `ValueError`.
4. Shipping is free when the amount after the discount and the coupon is at least 100.00; otherwise it is 7.00.
5. The total is the amount plus shipping, rounded to cents, half up (0.005 → 0.01).

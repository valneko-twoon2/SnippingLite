import unittest
from decimal import Decimal

from pricing import Order, price


class PriceTests(unittest.TestCase):
    def test_plain_order(self):
        self.assertIsNotNone(price(Order(Decimal("10"), 2)))

    def test_member(self):
        self.assertTrue(price(Order(Decimal("10"), 2, member=True)) > 0)

    def test_bulk(self):
        result = price(Order(Decimal("10"), 20))
        self.assertIsInstance(result, Decimal)

    def test_coupon(self):
        self.assertLess(price(Order(Decimal("30"), 3, coupon="SAVE5")), Decimal("1000"))

    def test_bad_coupon(self):
        with self.assertRaises(Exception):
            price(Order(Decimal("30"), 3, coupon="FREE"))

    def test_bad_quantity(self):
        with self.assertRaises(Exception):
            price(Order(Decimal("1"), 0))

    def test_negative_price(self):
        with self.assertRaises(Exception):
            price(Order(Decimal("-1"), 1))

    def test_big_order_free_shipping(self):
        self.assertTrue(price(Order(Decimal("60"), 2)) > 100)


if __name__ == "__main__":
    unittest.main()

# Copyright 2012 Martin Pool
# Licensed under the Apache License, Version 2.0 (the "License").

"""Unit test bands"""


from __future__ import absolute_import

from duralib.band import (
    cmp_band_numbers,
    _canonicalize_band_number,
    )
from duralib.tests.base import DuraTestCase


class TestBandNumbers(DuraTestCase):
    """Test formatting, parsing, sorting of band numbers."""

    def test_canonicalize_band_number(self):
        self.assertEqual("0000", _canonicalize_band_number("0"))
        self.assertEqual("0042", _canonicalize_band_number("42"))
        self.assertEqual("9999", _canonicalize_band_number("9999"))
        self.assertEqual("123456", _canonicalize_band_number("123456"))

    def test_cmp_band_number(self):
        self.assertEqual(-1, cmp_band_numbers("0000", "0001"))
        self.assertEqual(1, cmp_band_numbers("0900", "0001"))
        self.assertEqual(0, cmp_band_numbers("0900", "900"))
        self.assertEqual(-1, cmp_band_numbers("9000", "10001"))

    def test_sort_band_number(self):
        # Smart comparison, by number.
        numbers = ["0000", "0001", "0042", "9998", "9999", "10000", "12345",
        "990099"]
        self.assertEqual(
            numbers,
            sorted(numbers, cmp=cmp_band_numbers))
        self.assertEqual(
            numbers,
            sorted(sorted(numbers), cmp=cmp_band_numbers))
        self.assertEqual(
            numbers,
            sorted(sorted(numbers, reverse=True),
                cmp=cmp_band_numbers))

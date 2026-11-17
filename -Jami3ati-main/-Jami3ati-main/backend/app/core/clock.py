"""Naive UTC timestamps.

The existing columns are `DateTime` without a timezone, so storing an aware
datetime would silently change every comparison. This helper keeps the same
naive-UTC convention while avoiding `datetime.utcnow()`, which Python 3.12
deprecates.
"""

from datetime import datetime, timezone


def utcnow() -> datetime:
    return datetime.now(timezone.utc).replace(tzinfo=None)

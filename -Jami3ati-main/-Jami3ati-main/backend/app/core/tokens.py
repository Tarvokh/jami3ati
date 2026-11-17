"""Capability tokens for the Jami3ati subscription loop.

Two different jobs, deliberately kept apart:

* **Confirmation tokens** are bearer capabilities: whoever holds the link can
  confirm the subscription. Only a SHA-256 digest is stored, so a database
  leak does not hand out working links.
* **Unsubscribe tokens** are *derived*: an HMAC over the subscription id and
  the server secret. Nothing is stored, so an unsubscribe link can always be
  rebuilt from the row, and a leaked row cannot be replayed as a forgery
  because the secret never leaves the server.

Both are URL-safe and unpadded, so they survive a query string unchanged.
"""

import base64
import hashlib
import hmac
import secrets


def new_confirm_token() -> str:
    """A fresh 256-bit bearer token (43 URL-safe characters)."""
    return secrets.token_urlsafe(32)


def confirm_token_digest(token: str) -> str:
    """Digest stored in the row. Never store the token itself."""
    return hashlib.sha256(token.encode("utf-8")).hexdigest()


def _b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def unsubscribe_token(secret: str, subscription_id: str) -> str:
    """Deterministic token: same row + same secret = same token."""
    mac = hmac.new(
        secret.encode("utf-8"),
        f"unsub:{subscription_id}".encode("utf-8"),
        hashlib.sha256,
    ).digest()
    return _b64url(mac)


def unsubscribe_token_matches(secret: str, subscription_id: str, presented: str) -> bool:
    """Constant-time comparison; an empty or missing token never matches."""
    expected = unsubscribe_token(secret, subscription_id)
    return hmac.compare_digest(expected, presented or "")


def confirmation_link(base_url: str, subscription_id: str, token: str) -> str:
    base = base_url.rstrip("/")
    return f"{base}/api/v1/subscriptions/confirm?id={subscription_id}&token={token}"


def unsubscribe_link(base_url: str, secret: str, subscription_id: str) -> str:
    base = base_url.rstrip("/")
    token = unsubscribe_token(secret, subscription_id)
    return f"{base}/api/v1/subscriptions/unsubscribe?id={subscription_id}&token={token}"

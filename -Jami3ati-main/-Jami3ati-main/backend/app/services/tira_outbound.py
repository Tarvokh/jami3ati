"""The only place in Jami3ati that talks to Tira's machine send API.

Contract mirrored from Tira `crates/tira-api/src/send.rs`:

    POST {TIRA_OUTBOUND_URL}
    Authorization: Bearer <TIRA_OUTBOUND_TOKEN>
    {
      "mailbox_id": "<uuid>",
      "to": ["<address>"],
      "subject": "...",            # <= 255 chars, single line
      "text": "...",               # <= 64 KiB
      "idempotency_key": "...",    # 8-128 chars of [A-Za-z0-9._:-]
      "correlation_id": "...",     # optional, same charset
      "list_unsubscribe": "<https://...>",   # optional, must be https or mailto
      "reply_to": "..."            # optional, plain ASCII address
    }

Status mapping is the whole point of this module:

    202 -> accepted  (durable queue row, NOT delivery)
    400 -> rejected  (the request can never succeed; retrying is noise)
    404 -> rejected  (mailbox is not ours; retrying cannot fix that)
    429 -> retry     (throttled, the same request will work later)
    503 -> retry     (relay or storage down, the same request may work later)
    else -> retry    (unknown; do not silently drop a queued intent)
"""

from dataclasses import dataclass
from typing import Optional

import httpx

ACCEPTED = "accepted"
REJECTED = "rejected"
RETRY = "retry"


@dataclass(frozen=True)
class OutboundOutcome:
    kind: str
    detail: str = ""
    tira_message_id: Optional[str] = None


class TiraNotConfigured(RuntimeError):
    """Raised when the caller asked for a live send with no token/mailbox."""


class TiraOutboundClient:
    def __init__(
        self,
        url: str,
        token: str,
        mailbox_id: str,
        timeout_seconds: float = 10.0,
    ) -> None:
        if not token or not mailbox_id:
            raise TiraNotConfigured(
                "TIRA_OUTBOUND_TOKEN and TIRA_MAILBOX_ID are both required"
            )
        self.url = url
        self.token = token
        self.mailbox_id = mailbox_id
        self.timeout_seconds = timeout_seconds

    def send(
        self,
        *,
        to: str,
        subject: str,
        text: str,
        idempotency_key: str,
        correlation_id: str,
        list_unsubscribe: Optional[str] = None,
        reply_to: Optional[str] = None,
    ) -> OutboundOutcome:
        payload = {
            "mailbox_id": self.mailbox_id,
            "to": [to],
            "subject": subject,
            "text": text,
            "idempotency_key": idempotency_key,
            "correlation_id": correlation_id,
        }
        if list_unsubscribe:
            payload["list_unsubscribe"] = list_unsubscribe
        if reply_to:
            payload["reply_to"] = reply_to

        try:
            response = httpx.post(
                self.url,
                json=payload,
                headers={"Authorization": f"Bearer {self.token}"},
                timeout=self.timeout_seconds,
            )
        except httpx.HTTPError as exc:
            # A transport failure is not a refusal: the queue row stays alive.
            return OutboundOutcome(RETRY, f"transport: {type(exc).__name__}")

        if response.status_code == 202:
            body = _json_or_empty(response)
            return OutboundOutcome(
                ACCEPTED,
                "accepted",
                body.get("id") if isinstance(body, dict) else None,
            )

        if response.status_code in (400, 404):
            return OutboundOutcome(REJECTED, _describe(response))

        return OutboundOutcome(RETRY, _describe(response))


def _json_or_empty(response: httpx.Response):
    try:
        return response.json()
    except ValueError:
        return {}


def _describe(response: httpx.Response) -> str:
    text = (response.text or "").strip().replace("\n", " ")
    return f"http {response.status_code}: {text[:200]}"

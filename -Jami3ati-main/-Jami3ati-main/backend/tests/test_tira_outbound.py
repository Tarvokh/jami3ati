"""Contract tests for the Jami3ati -> Tira boundary.

No network: `httpx.post` is replaced. What is pinned here is the *shape* the
Rust side validates (`crates/tira-api/src/send.rs`) and the status mapping,
because a mismatch would only surface on stage otherwise.
"""

import re

import httpx
import pytest

from app.services.tira_outbound import (
    ACCEPTED,
    REJECTED,
    RETRY,
    TiraNotConfigured,
    TiraOutboundClient,
)

MAILBOX = "33333333-3333-4333-8333-333333333333"
# Tira: "idempotency_key must be 8-128 chars of [A-Za-z0-9._:-]"
KEY_CHARSET = re.compile(r"^[A-Za-z0-9._:-]{8,128}$")
# Tira's `optional_key` (correlation_id) allows 1-128, not 8-128.
OPTIONAL_KEY_CHARSET = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")


class _Response:
    def __init__(self, status_code, payload=None, text=""):
        self.status_code = status_code
        self._payload = payload
        self.text = text

    def json(self):
        if self._payload is None:
            raise ValueError("not json")
        return self._payload


def _client():
    return TiraOutboundClient(
        "http://tira.invalid/api/v1/outbound", "test-token", MAILBOX
    )


def _capture(monkeypatch, response=None, exc=None):
    seen = {}

    def fake_post(url, json=None, headers=None, timeout=None):
        seen["url"] = url
        seen["json"] = json
        seen["headers"] = headers
        seen["timeout"] = timeout
        if exc is not None:
            raise exc
        return response

    monkeypatch.setattr(httpx, "post", fake_post)
    return seen


def _send(client, **overrides):
    payload = {
        "to": "student@uit.ac.ma",
        "subject": "فتح الترشيح",
        "text": "نص الرسالة",
        "idempotency_key": "jami3ati:ntf-abc:sub-def",
        "correlation_id": "ntf-abc",
    }
    payload.update(overrides)
    return client.send(**payload)


def test_client_refuses_to_start_without_credentials():
    with pytest.raises(TiraNotConfigured):
        TiraOutboundClient("http://x", "", MAILBOX)
    with pytest.raises(TiraNotConfigured):
        TiraOutboundClient("http://x", "token", "")


def test_payload_matches_the_closed_contract(monkeypatch):
    seen = _capture(
        monkeypatch,
        _Response(202, {"id": "0b7f0f0e-0000-4000-8000-000000000000", "status": "queued", "duplicate": False}),
    )

    outcome = _send(_client(), list_unsubscribe="<https://jami3ati.ma/u/1>", reply_to="contact@jami3ati.ma")

    assert outcome.kind == ACCEPTED
    assert seen["url"] == "http://tira.invalid/api/v1/outbound"
    assert seen["headers"]["Authorization"] == "Bearer test-token"
    body = seen["json"]
    assert set(body) == {
        "mailbox_id",
        "to",
        "subject",
        "text",
        "idempotency_key",
        "correlation_id",
        "list_unsubscribe",
        "reply_to",
    }
    assert body["mailbox_id"] == MAILBOX
    assert body["to"] == ["student@uit.ac.ma"]
    assert KEY_CHARSET.match(body["idempotency_key"])
    assert OPTIONAL_KEY_CHARSET.match(body["correlation_id"])


def test_optional_headers_are_omitted_not_sent_as_null(monkeypatch):
    seen = _capture(monkeypatch, _Response(202, {"id": "x", "status": "queued", "duplicate": False}))
    _send(_client())
    assert "list_unsubscribe" not in seen["json"]
    assert "reply_to" not in seen["json"]


def test_202_returns_the_tira_message_id(monkeypatch):
    _capture(
        monkeypatch,
        _Response(202, {"id": "0b7f0f0e-0000-4000-8000-000000000000", "status": "queued", "duplicate": False}),
    )
    outcome = _send(_client())
    assert outcome.kind == ACCEPTED
    assert outcome.tira_message_id == "0b7f0f0e-0000-4000-8000-000000000000"


@pytest.mark.parametrize("status", [400, 404])
def test_client_errors_are_rejected_and_never_retried(monkeypatch, status):
    _capture(monkeypatch, _Response(status, {"error": "nope"}))
    outcome = _send(_client())
    assert outcome.kind == REJECTED
    assert str(status) in outcome.detail


@pytest.mark.parametrize("status", [429, 500, 502, 503])
def test_server_errors_are_retried(monkeypatch, status):
    _capture(monkeypatch, _Response(status, {"error": "later"}))
    outcome = _send(_client())
    assert outcome.kind == RETRY
    assert str(status) in outcome.detail


def test_transport_failure_is_retried(monkeypatch):
    _capture(monkeypatch, exc=httpx.ConnectError("refused"))
    outcome = _send(_client())
    assert outcome.kind == RETRY
    assert "ConnectError" in outcome.detail


def test_a_non_json_202_still_counts_as_accepted(monkeypatch):
    _capture(monkeypatch, _Response(202, None, text=""))
    outcome = _send(_client())
    assert outcome.kind == ACCEPTED
    assert outcome.tira_message_id is None

"""Subscription endpoints — double opt-in, and nothing less.

There is no "subscribe" that adds an address directly. Every new address is
`pending` until the holder of the mailbox opens the confirmation link. The
worker delivers that link through the same outbox as every other message, so
the demo exercises the real path rather than a shortcut.
"""

import logging
import uuid
from datetime import timedelta
from typing import Optional

from fastapi import APIRouter, Depends, HTTPException, Query, Response
from sqlalchemy.orm import Session

from app.core import tokens
from app.core.clock import utcnow
from app.core.config import settings
from app.core.database import get_db
from app.models.subscription import SubscriptionModel
from app.schemas.subscription import (
    SubscriptionCreate,
    SubscriptionResponse,
    SubscriptionStatusResponse,
)
from app.services import deliveries

logger = logging.getLogger("jami3ati.subscriptions")

router = APIRouter(prefix="/subscriptions", tags=["Subscriptions"])


def _hmac_secret() -> str:
    secret = settings.JAMI3ATI_HMAC_SECRET.strip()
    if not secret:
        # Fail closed. Without the secret no unsubscribe link can be built or
        # verified, and a subscription without a working unsubscribe link is
        # not something we are allowed to collect.
        raise HTTPException(
            status_code=503,
            detail="JAMI3ATI_HMAC_SECRET غير مضبوط؛ خدمة الاشتراك معطّلة.",
        )
    return secret


def _issue_confirmation(
    db: Session, subscription: SubscriptionModel, *, base_url: str
) -> None:
    raw_token = tokens.new_confirm_token()
    ttl = max(settings.CONFIRM_TOKEN_TTL_HOURS, 1)
    subscription.confirm_token_hash = tokens.confirm_token_digest(raw_token)
    subscription.confirm_expires_at = utcnow() + timedelta(hours=ttl)
    deliveries.enqueue_confirmation(
        db, subscription, base_url=base_url, raw_token=raw_token
    )


@router.post("", response_model=SubscriptionResponse, status_code=201)
def subscribe(
    payload: SubscriptionCreate,
    response: Response,
    db: Session = Depends(get_db),
):
    _hmac_secret()
    base_url = settings.JAMI3ATI_PUBLIC_BASE_URL
    existing = (
        db.query(SubscriptionModel)
        .filter(SubscriptionModel.email == payload.email)
        .first()
    )

    if existing is not None:
        # Idempotent signup: never create a second row for the same address.
        response.status_code = 200
        if existing.status == "active":
            return existing
        # pending or unsubscribed: rotate the token and resend. The old link
        # stops working the moment the digest is replaced.
        existing.status = "pending"
        existing.institution_id = payload.institution_id
        existing.unsubscribed_at = None
        _issue_confirmation(db, existing, base_url=base_url)
        db.commit()
        db.refresh(existing)
        return existing

    subscription = SubscriptionModel(
        id=f"sub-{uuid.uuid4().hex[:12]}",
        email=payload.email,
        institution_id=payload.institution_id,
        status="pending",
    )
    db.add(subscription)
    _issue_confirmation(db, subscription, base_url=base_url)
    db.commit()
    db.refresh(subscription)
    return subscription


@router.get("/confirm", response_model=SubscriptionStatusResponse)
def confirm(
    id: str = Query(..., min_length=1),
    token: str = Query(..., min_length=1),
    db: Session = Depends(get_db),
):
    _hmac_secret()
    subscription = db.query(SubscriptionModel).filter(SubscriptionModel.id == id).first()
    if subscription is None:
        # Same body for unknown id and bad token: the id space must not be
        # enumerable from this endpoint.
        raise HTTPException(status_code=400, detail="رابط التأكيد غير صالح أو منتهي.")

    if subscription.status == "active":
        return SubscriptionStatusResponse(id=subscription.id, status="active")

    stored = subscription.confirm_token_hash or ""
    presented = tokens.confirm_token_digest(token)
    if not stored or not _digest_equal(stored, presented):
        raise HTTPException(status_code=400, detail="رابط التأكيد غير صالح أو منتهي.")

    if subscription.confirm_expires_at is None or subscription.confirm_expires_at < utcnow():
        raise HTTPException(status_code=400, detail="رابط التأكيد غير صالح أو منتهي.")

    subscription.status = "active"
    subscription.confirmed_at = utcnow()
    subscription.confirm_token_hash = None
    subscription.confirm_expires_at = None
    db.commit()
    return SubscriptionStatusResponse(id=subscription.id, status="active")


def _unsubscribe(db: Session, id: str, token: str) -> SubscriptionStatusResponse:
    secret = _hmac_secret()
    subscription = db.query(SubscriptionModel).filter(SubscriptionModel.id == id).first()
    if subscription is None:
        raise HTTPException(status_code=400, detail="رابط إلغاء الاشتراك غير صالح.")

    if not tokens.unsubscribe_token_matches(secret, subscription.id, token):
        raise HTTPException(status_code=400, detail="رابط إلغاء الاشتراك غير صالح.")

    if subscription.status != "unsubscribed":
        subscription.status = "unsubscribed"
        subscription.unsubscribed_at = utcnow()
        subscription.confirm_token_hash = None
        subscription.confirm_expires_at = None
        db.commit()
    return SubscriptionStatusResponse(id=subscription.id, status="unsubscribed")


@router.get("/unsubscribe", response_model=SubscriptionStatusResponse)
def unsubscribe_get(
    id: str = Query(..., min_length=1),
    token: str = Query(..., min_length=1),
    db: Session = Depends(get_db),
):
    return _unsubscribe(db, id, token)


@router.post("/unsubscribe", response_model=SubscriptionStatusResponse)
def unsubscribe_one_click(
    id: str = Query(..., min_length=1),
    token: str = Query(..., min_length=1),
    db: Session = Depends(get_db),
):
    # RFC 8058 one-click: the mail client POSTs to the List-Unsubscribe URL.
    # The token is in the query string because that is the URL it was given.
    return _unsubscribe(db, id, token)


def _digest_equal(left: str, right: str) -> bool:
    import hmac as _hmac

    return _hmac.compare_digest(left, right)
